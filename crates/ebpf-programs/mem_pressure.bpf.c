#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>

char LICENSE[] SEC("license") = "GPL";

struct mem_pressure_event {
    u64 pages_reclaimed;
    u32 zone;
};

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 64 * 1024);
} events_ringbuf SEC(".maps");

// Tracepoint argument layout for shrink inactive success to capture real page reclaims (§3.3, BUG-P1)
struct trace_event_raw_mm_vmscan_shrink_inactive_to_succ {
    unsigned short common_type;
    unsigned char common_flags;
    unsigned char common_preempt_count;
    int common_pid;
    int nid;
    unsigned long nr_scanned;
    unsigned long nr_reclaimed; // Number of reclaimed pages
    int priority;
    int reclaim_flags;
} __attribute__((preserve_access_index));

SEC("tracepoint/vmscan/mm_vmscan_shrink_inactive_to_succ")
int handle_mm_vmscan(void *ctx) {
    struct mem_pressure_event *e;
    struct trace_event_raw_mm_vmscan_shrink_inactive_to_succ *args = ctx;

    e = bpf_ringbuf_reserve(&events_ringbuf, sizeof(*e), 0);
    if (!e) return 0;

    // Capture the real number of reclaimed pages from the kernel tracepoint args (§3.3)
    e->pages_reclaimed = args->nr_reclaimed;
    e->zone = args->nid; // Node ID acts as the zone indicator

    bpf_ringbuf_submit(e, 0);
    return 0;
}
