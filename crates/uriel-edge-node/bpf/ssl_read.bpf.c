// ssl_read.bpf.c
// eBPF UProbe — attaches to SSL_read() in libssl.so.
// Captures decrypted plaintext payloads in memory before they leave the TLS layer.
// Compile: clang -O2 -target bpf -c ssl_read.bpf.c -o target/bpf/ssl_read.bpf.o
// Requires: linux-headers, clang, bpftool, libssl-dev

#include <linux/bpf.h>
#include <linux/ptrace.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

#define MAX_BUF_LEN 512

struct ssl_event {
    __u32 pid;
    __u32 len;
    char  comm[16];
    char  data[MAX_BUF_LEN];
};

// Ring buffer map — Aya reads from this in userspace
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1024 * 1024);
} ssl_events SEC(".maps");

// SSL_read(SSL *ssl, void *buf, int num) -> int
// arg0 = ssl*, arg1 = buf*, arg2 = num
// We hook the RETURN (uretprobe) so we know how many bytes were actually read.
SEC("uretprobe/SSL_read")
int ssl_read_hook(struct pt_regs *ctx) {
    int ret = (int)PT_REGS_RC(ctx);
    if (ret <= 0)
        return 0;  // nothing read or error

    // Get the buf pointer from arg1 (stored in BPF context via uprobe map)
    // NOTE: For a full implementation use a map to stash arg1 from the uprobe entry.
    // This simplified version reads from the stack-preserved register on x86-64.
    void *buf = (void *)PT_REGS_PARM2(ctx);

    struct ssl_event *e = bpf_ringbuf_reserve(&ssl_events, sizeof(*e), 0);
    if (!e)
        return 0;

    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->len = (__u32)(ret < MAX_BUF_LEN ? ret : MAX_BUF_LEN);
    bpf_get_current_comm(&e->comm, sizeof(e->comm));
    bpf_probe_read_user(e->data, e->len, buf);

    bpf_ringbuf_submit(e, 0);
    return 0;
}

char _license[] SEC("license") = "GPL";
