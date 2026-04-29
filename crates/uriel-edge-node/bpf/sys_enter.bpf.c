// sys_enter.bpf.c
// eBPF KProbe — intercepts every sys_enter syscall.
// Logs process name + syscall number for rogue socket detection.
// Compile: clang -O2 -target bpf -c sys_enter.bpf.c -o target/bpf/sys_enter.bpf.o
// Requires: linux-headers, clang, bpftool

#include <linux/bpf.h>
#include <linux/ptrace.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

// Map: ring buffer for events sent to userspace (Aya reads this)
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024);
} events SEC(".maps");

struct sys_event {
    __u32 pid;
    __u32 syscall_nr;
    char  comm[16];
};

// Suspicious socket-related syscall numbers (x86-64)
// 41 = socket, 42 = connect, 49 = bind, 50 = listen, 288 = accept4
#define SOCKET_SYSCALL  41
#define CONNECT_SYSCALL 42
#define BIND_SYSCALL    49

SEC("kprobe/sys_enter")
int sys_enter(struct pt_regs *ctx) {
    __u64 nr = PT_REGS_PARM1(ctx);

    // Only capture socket-related syscalls (reduces overhead significantly)
    if (nr != SOCKET_SYSCALL && nr != CONNECT_SYSCALL && nr != BIND_SYSCALL)
        return 0;

    struct sys_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;

    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->syscall_nr = (__u32)nr;
    bpf_get_current_comm(&e->comm, sizeof(e->comm));

    bpf_ringbuf_submit(e, 0);
    return 0;
}

char _license[] SEC("license") = "GPL";
