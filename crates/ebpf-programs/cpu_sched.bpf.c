#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>

char LICENSE[] SEC("license") = "GPL";

struct cpu_sched_event {
    u64 prev_comm_hash;
    u64 next_comm_hash;
    u64 latency_ns;
    u32 cpu_id;
};

// Define the BPF ring buffer map
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024); // 256 KB ring buffer
} events_ringbuf SEC(".maps");

// BPF Hash map to measure real scheduling latency (§3.3, BUG-07)
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 10240);
    __type(key, u32);   // Task PID
    __type(val, u64);   // Wakeup timestamp in nanoseconds
} wakeup_timestamps SEC(".maps");

// Simple FNV-1a 64-bit hash in kernel space for privacy-by-design
static __always_inline u64 fnv1a_64_hash(const char *str) {
    u64 hash = 0xcbf29ce484222325ULL;
    for (int i = 0; i < 16; i++) {
        if (str[i] == '\0') break;
        hash ^= (u64)(str[i]);
        hash *= 0x100000001b3ULL;
    }
    return hash;
}

// Struct declarations for reading next task names in sched_switch
struct trace_event_raw_sched_switch {
    unsigned short common_type;
    unsigned char common_flags;
    unsigned char common_preempt_count;
    int common_pid;
    char prev_comm[16];
    int prev_pid;
    int prev_prio;
    long long prev_state;
    char next_comm[16];
    int next_pid;
    int next_prio;
} __attribute__((preserve_access_index));

// Struct declarations for reading wakeup PID in sched_wakeup
struct trace_event_raw_sched_wakeup {
    unsigned short common_type;
    unsigned char common_flags;
    unsigned char common_preempt_count;
    int common_pid;
    char comm[16];
    int pid;
    int prio;
    int success;
    int target_cpu;
} __attribute__((preserve_access_index));

// Struct declarations for sched_process_exit cleanup (§12.1, BUG-P2)
struct trace_event_raw_sched_process_exit {
    unsigned short common_type;
    unsigned char common_flags;
    unsigned char common_preempt_count;
    int common_pid;
    char comm[16];
    int pid;
    int prio;
} __attribute__((preserve_access_index));

SEC("tracepoint/sched/sched_wakeup")
int handle_sched_wakeup(void *ctx) {
    struct trace_event_raw_sched_wakeup *args = ctx;
    u32 pid = args->pid;
    u64 ts = bpf_ktime_get_ns();

    // Record wakeup timestamp
    bpf_map_update_elem(&wakeup_timestamps, &pid, &ts, 0);
    return 0;
}

SEC("tracepoint/sched/sched_switch")
int handle_sched_switch(void *ctx) {
    struct cpu_sched_event *e;
    struct trace_event_raw_sched_switch *args = ctx;

    e = bpf_ringbuf_reserve(&events_ringbuf, sizeof(*e), 0);
    if (!e) {
        return 0;
    }

    char prev_comm[16] = {};
    char next_comm[16] = {};
    
    bpf_probe_read_kernel_str(&prev_comm, sizeof(prev_comm), args->prev_comm);
    bpf_probe_read_kernel_str(&next_comm, sizeof(next_comm), args->next_comm);
    
    e->prev_comm_hash = fnv1a_64_hash(prev_comm);
    e->next_comm_hash = fnv1a_64_hash(next_comm);
    e->cpu_id = bpf_get_smp_processor_id();
    
    // Calculate actual queue latency delta for the task being scheduled (§3.3, BUG-07)
    u32 next_pid = args->next_pid;
    u64 *wakeup_ts = bpf_map_lookup_elem(&wakeup_timestamps, &next_pid);
    if (wakeup_ts) {
        u64 now = bpf_ktime_get_ns();
        if (now >= *wakeup_ts) {
            e->latency_ns = now - *wakeup_ts;
        } else {
            e->latency_ns = 0;
        }
        bpf_map_delete_elem(&wakeup_timestamps, &next_pid);
    } else {
        e->latency_ns = 0; // Fallback to 0 if wakeup wasn't captured
    }

    bpf_ringbuf_submit(e, 0);
    return 0;
}

// Cleanup wakeup map entries for terminated processes to prevent unbounded saturation (§12.1, BUG-P2)
SEC("tracepoint/sched/sched_process_exit")
int handle_process_exit(void *ctx) {
    struct trace_event_raw_sched_process_exit *args = ctx;
    u32 pid = args->pid;
    bpf_map_delete_elem(&wakeup_timestamps, &pid);
    return 0;
}
