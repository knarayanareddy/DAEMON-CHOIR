#include "vmlinux.h"
#include <bpf/bpf_helpers.h>

char LICENSE[] SEC("license") = "GPL";

struct proc_lifecycle_event {
    u64 comm_hash;
    u64 uid_hash;
};

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 64 * 1024);
} events_ringbuf SEC(".maps");

static __always_inline u64 fnv1a_64_hash(const char *str) {
    u64 hash = 0xcbf29ce484222325ULL;
    for (int i = 0; i < 16; i++) {
        if (str[i] == '\0') break;
        hash ^= (u64)(str[i]);
        hash *= 0x100000001b3ULL;
    }
    return hash;
}

// Inline FNV-1a 64-bit hash for a 32-bit integer to secure raw UIDs
static __always_inline u64 fnv1a_64_hash_u32(u32 val) {
    u64 hash = 0xcbf29ce484222325ULL;
    hash ^= (val & 0xFF);
    hash *= 0x100000001b3ULL;
    hash ^= ((val >> 8) & 0xFF);
    hash *= 0x100000001b3ULL;
    hash ^= ((val >> 16) & 0xFF);
    hash *= 0x100000001b3ULL;
    hash ^= ((val >> 24) & 0xFF);
    hash *= 0x100000001b3ULL;
    return hash;
}

SEC("tracepoint/sched/sched_process_exec")
int handle_process_exec(void *ctx) {
    struct proc_lifecycle_event *e;

    e = bpf_ringbuf_reserve(&events_ringbuf, sizeof(*e), 0);
    if (!e) return 0;

    char comm[16] = {};
    bpf_get_current_comm(&comm, sizeof(comm));
    
    e->comm_hash = fnv1a_64_hash(comm);
    
    // Hash the UID inside the kernel prior to ring buffer submission (§10.2, §5.1)
    u32 uid = bpf_get_current_uid_gid() & 0xFFFFFFFF;
    e->uid_hash = fnv1a_64_hash_u32(uid);

    bpf_ringbuf_submit(e, 0);
    return 0;
}
