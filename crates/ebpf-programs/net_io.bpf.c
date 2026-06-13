#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>

char LICENSE[] SEC("license") = "GPL";

struct net_io_event {
    u64 bytes_tx;
    u64 bytes_rx;
    u64 dev_name_hash;
};

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 128 * 1024);
} events_ringbuf SEC(".maps");

// Simple FNV-1a 64-bit hash in kernel space for device name privacy
static __always_inline u64 fnv1a_64_hash(const char *str) {
    u64 hash = 0xcbf29ce484222325ULL;
    for (int i = 0; i < 16; i++) {
        if (str[i] == '\0') break;
        hash ^= (u64)(str[i]);
        hash *= 0x100000001b3ULL;
    }
    return hash;
}

// Tracepoint argument layout for net_dev_xmit to capture real bytes tx (§3.3, BUG-P1)
struct trace_event_raw_net_dev_xmit {
    unsigned short common_type;
    unsigned char common_flags;
    unsigned char common_preempt_count;
    int common_pid;
    void *skbaddr;
    unsigned int len; // Actual packet length
    int rc;
    char name[32];    // Device name
} __attribute__((preserve_access_index));

SEC("tracepoint/net/net_dev_xmit")
int handle_net_dev_xmit(void *ctx) {
    struct net_io_event *e;
    struct trace_event_raw_net_dev_xmit *args = ctx;

    e = bpf_ringbuf_reserve(&events_ringbuf, sizeof(*e), 0);
    if (!e) return 0;

    // Capture the real transmitted bytes length from the kernel tracepoint args (§3.3)
    e->bytes_tx = args->len;
    e->bytes_rx = 0; // Rx is tx-only on xmit; receiver handles Rx separately

    char dev_name[16] = {};
    bpf_probe_read_kernel_str(&dev_name, sizeof(dev_name), args->name);
    e->dev_name_hash = fnv1a_64_hash(dev_name);

    bpf_ringbuf_submit(e, 0);
    return 0;
}
