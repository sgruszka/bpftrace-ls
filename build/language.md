<!--
This file originates from https://github.com/bpftrace/bpftrace
It is licensed under the Apache License, Version 2.0
See the LICENSE file of that project or https://www.apache.org/licenses/LICENSE-2.0
-->
## Probes

bpftrace supports various probe types which allow the user to attach BPF programs to different types of events.
Each probe starts with a provider (e.g. `kprobe`) followed by a colon (`:`) separated list of options.
An optional name may precede the provider with an equals sign (e.g. `name=`), which is reserved for internal use and future features.
The amount of options and their meaning depend on the provider and are detailed below.
The valid values for options can depend on the system or binary being traced, e.g. for uprobes it depends on the binary.
Also see [Listing Probes](../man/adoc/bpftrace.adoc#listing-probes).

It is possible to associate multiple probes with a single action as long as the action is valid for all specified probes.
Multiple probes can be specified as a comma (`,`) separated list:

```
kprobe:tcp_reset,kprobe:tcp_v4_rcv {
  printf("Entered: %s\n", probe);
}
```

Wildcards are supported too:

```
kprobe:tcp_* {
  printf("Entered: %s\n", probe);
}
```

Both can be combined:

```
kprobe:tcp_reset,kprobe:*socket* {
  printf("Entered: %s\n", probe);
}
```

By default, bpftrace requires all probes to attach successfully or else an error is returned. However this can be changed using the `missing_probes` config variable.

Most providers also support a short name which can be used instead of the full name, e.g. `kprobe:f` and `k:f` are identical.

|     |     |     |
| --- | --- | --- |
| **Probe Name** | **Short Name** | **Description** |
| [`begin/end`](#beginend) | - | Built-in events |
| [`bench`](#bench) | - | Micro benchmarks |
| [`self`](#self) | - | Built-in events |
| [`hardware`](#hardware) | `h` | Processor-level events |
| [`interval`](#interval) | `i` | Timed output |
| [`iter`](#iterator) | `it` | Iterators tracing |
| [`fentry/fexit`](#fentry-and-fexit) | `f`/`fr` | Kernel functions tracing with BTF support |
| [`kprobe/kretprobe`](#kprobe-and-kretprobe) | `k`/`kr` | Kernel function start/return |
| [`profile`](#profile) | `p` | Timed sampling |
| [`rawtracepoint`](#rawtracepoint) | `rt` | Kernel static tracepoints with raw arguments |
| [`software`](#software) | `s` | Kernel software events |
| [`tracepoint`](#tracepoint) | `t` | Kernel static tracepoints |
| [`uprobe/uretprobe`](#uprobe-uretprobe) | `u`/`ur` | User-level function start/return |
| [`usdt`](#usdt) | `U` | User-level static tracepoints |
| [`watchpoint/asyncwatchpoint`](#watchpoint-and-asyncwatchpoint) | `w`/`aw` | Memory watchpoints |

### begin
Special built-in event provided by the bpftrace runtime.
`begin` is triggered before all other probes are attached.
Can be used any number of times, and they will be executed in the same order they are declared.

### end
Special built-in event provided by the bpftrace runtime.
`end` is triggered after all other probes are detached.
Each of these probes can be used any number of times, and they will be executed in the same order they are declared.

#### Notes
Specifying an `end` probe doesn’t override the printing of 'non-empty' maps at exit.
To prevent printing all used maps need be cleared in the `end` probe:

#### Example
```
end {
    clear(@map1);
    clear(@map2);
}
```

### test

`test` is a special built-in probe type for creating tests.
bpftrace executes each `test` probe and checks the return value, error count and possible exit calls to determine a pass.
If multiple `test` probes exist in a script, bpftrace executes them sequentially in the order they are specified.
To run `test` probes, you must run bpftrace in test mode: `bpftrace --test ...`; otherwise `test` probes will be ignored.

```
test:okay {
  print("I'm okay! This output will be suppressed.");
}

test:failure {
  print("This is a failure! This output will be shown");
  return 1;
}
```

### bench

`bench` is a special built-in probe type for creating micro benchmarks.
bpftrace executes each `bench` probe repeatedly to measure the average execution time of the contained code.
If multiple `bench` probes exist in a script, bpftrace executes them sequentially in the order they are specified.
To run `bench` probes, you must run bpftrace in bench mode: `bpftrace --bench ...`; otherwise, `bench` probes will be ignored.

```
bench:lhist {
    @a = lhist(rand % 10, 1, 10, 1);
}

bench:count {
    @b = count();
}

bench:my_loop {
    $a = 0;
    for ($i : 0..10) {
        $a++;
    }
}
```

```
Attached 3 probes


+-----------+-------------+
| BENCHMARK | NANOSECONDS |
+-----------+-------------+
| count     | 40          |
| lhist     | 88          |
| my_loop   | 124         |
+-----------+-------------+
```

### self

**variants**

* `self:signal:SIGUSR1`

These are special built-in events provided by the bpftrace runtime.
The trigger function is called by the bpftrace runtime when the bpftrace process receives specific events, such as a `SIGUSR1` signal.

```
self:signal:SIGUSR1 {
  print("abc");
}
```

### hardware

**variants**

* `hardware:event_name:`
* `hardware:event_name:count`

**short name**

* `h`

These are the pre-defined hardware events provided by the Linux kernel, as commonly traced by the perf utility.
They are implemented using performance monitoring counters (PMCs): hardware resources on the processor.
There are about ten of these, and they are documented in the perf_event_open(2) man page.
The event names are:

* `cpu-cycles` or `cycles`
* `instructions`
* `cache-references`
* `cache-misses`
* `branch-instructions` or `branches`
* `branch-misses`
* `bus-cycles`
* `frontend-stalls`
* `backend-stalls`
* `ref-cycles`

The `count` option specifies how many events must happen before the probe fires (sampling interval).
If `count` is left unspecified a default value is used.

This will fire once for every 1,000,000 cache misses.

```
hardware:cache-misses:1e6 { @[pid] = count(); }
```

### interval

**variants**

* `interval:count`
* `interval:us:count`
* `interval:ms:count`
* `interval:s:count`
* `interval:hz:rate`

**short name**

* `i`

The interval probe fires at a fixed interval as specified by its time spec.
Interval fires on one CPU at a time, unlike [profile](#profile) probes.
If a unit of time is not specified in the second position, the number is interpreted as nanoseconds; e.g., `interval:1s`, `interval:1000000000`, and `interval:s:1` are all equivalent.

This prints the rate of syscalls per second.

```
tracepoint:raw_syscalls:sys_enter { @syscalls = count(); }
interval:1s { print(@syscalls); clear(@syscalls); }
```

### iter

**variants**

* `iter:task`
* `iter:task:pin`
* `iter:task_file`
* `iter:task_file:pin`
* `iter:task_vma`
* `iter:task_vma:pin`

**short name**

* `it`

***Warning*** this feature is experimental and may be subject to interface changes.

These are eBPF iterator probes that allow iteration over kernel objects.
Iterator probe can’t be mixed with any other probe, not even another iterator.
Each iterator probe provides a set of fields that could be accessed with the
ctx pointer. Users can display the set of available fields for each iterator via
-lv options as described below.

```
iter:task { printf("%s:%d\n", ctx.task.comm, ctx.task.pid); }

/*
 * Sample output:
 * systemd:1
 * kthreadd:2
 * rcu_gp:3
 * rcu_par_gp:4
 * kworker/0:0H:6
 * mm_percpu_wq:8
 */
```

```
iter:task_file {
  printf("%s:%d %d:%s\n", ctx.task.comm, ctx.task.pid, ctx.fd, path(ctx.file.f_path));
}

/*
 * Sample output:
 * systemd:1 1:/dev/null
 * systemd:1 3:/dev/kmsg
 * ...
 * su:1622 2:/dev/pts/1
 * ...
 * bpftrace:1892 2:/dev/pts/1
 * bpftrace:1892 6:anon_inode:bpf-prog
 */
```

```
iter:task_vma {
  printf("%s %d %lx-%lx\n", comm, pid, ctx.vma.vm_start, ctx.vma.vm_end);
}

/*
 * Sample output:
 * bpftrace 119480 55b92c380000-55b92c386000
 * ...
 * bpftrace 119480 7ffd55dde000-7ffd55de2000
 */
```

It’s possible to pin an iterator by specifying the optional probe ':pin' part, that defines the pin file.
It can be specified as an absolute or relative path to /sys/fs/bpf.

**relative pin**

```
iter:task:list { printf("%s:%d\n", ctx.task.comm, ctx.task.pid); }

/*
 * Sample output:
 * Program pinned to /sys/fs/bpf/list
 */
```

**absolute pin**

```
iter:task_file:/sys/fs/bpf/files {
  printf("%s:%d %s\n", ctx.task.comm, ctx.task.pid, path(ctx.file.f_path));
}

/*
 * Sample output:
 * Program pinned to /sys/fs/bpf/files
 */
```

### fentry
* `fentry[:module]:fn`
* `fentry:bpf[:prog_id]:prog_name`

**short names**

* `f` (`fentry`)

``fentry``/``fexit`` probes attach to kernel functions similar to [kprobe and kretprobe](#kprobe-and-kretprobe).
They make use of eBPF trampolines which allow kernel code to call into BPF programs with near zero overhead.
Originally, these were called `kfunc` and `kretfunc` but were later renamed to `fentry` and `fexit` to match
how these are referenced in the kernel and to prevent confusion with [BPF Kernel Functions](https://docs.kernel.org/bpf/kfuncs.html).
The original names are still supported for backwards compatibility.

``fentry``/``fexit`` probes make use of BTF type information to derive the type of function arguments at compile time.
This removes the need for manual type casting and makes the code more resilient against small signature changes in the kernel.
The function arguments are available in the `args` struct which can be inspected by doing verbose listing (see [Listing Probes](../man/adoc/bpftrace.adoc#listing-probes)).
These arguments are also available in the return probe (`fexit`), unlike `kretprobe`.

The bpf variants (e.g. `fentry:bpf[:prog_id]:prog_name`) allow attaching to running BPF programs and sub-programs.
For example, if bpftrace was already running with a script like `uprobe:./testprogs/uprobe_test:uprobeFunction1 { print("hello"); }` then you could attach to this program with `fexit:bpf:uprobe___testprogs_uprobe_test_uprobeFunction1_1 { print("bye"); }` and this probe would execute after (because it's `fexit`) the `print("hello")` probe executes.
You can specify just the program name, and in this case bpftrace will attach to all running programs and sub-programs with that name.
You can differentiate between them using the `probe` builtin.
You can also specify the program id (e.g. `fentry:bpf:123:*`) to attach to a specific running BPF program or sub-programs called in that running BPF program.
To see a list of running, valid BPF programs and sub-programs use `bpftrace -l 'fentry:bpf:*'`.
Note: only BPF programs with a BTF Id can be attached to.
Also, the `args` builtin is not yet available for this variant.

#### Examples
```
# bpftrace -lv 'fentry:tcp_reset'

fentry:tcp_reset
    struct sock * sk
    struct sk_buff * skb
```

```
fentry:x86_pmu_stop {
  printf("pmu %s stop\n", str(args.event.pmu.name));
}
```
### fexit

**variants**
* `fexit[:module]:fn`
* `fexit:bpf[:prog_id]:prog_name`

**short names**
* `fr`

``fentry``/``fexit`` probes attach to kernel functions similar to [kprobe and kretprobe](#kprobe-and-kretprobe).
They make use of eBPF trampolines which allow kernel code to call into BPF programs with near zero overhead.
Originally, these were called `kfunc` and `kretfunc` but were later renamed to `fentry` and `fexit` to match
how these are referenced in the kernel and to prevent confusion with [BPF Kernel Functions](https://docs.kernel.org/bpf/kfuncs.html).
The original names are still supported for backwards compatibility.

The fget function takes one argument as file descriptor and you can access it via args.fd and the return value is accessible via retval:

```
fexit:fget {
  printf("fd %d name %s\n", args.fd, str(retval.f_path.dentry.d_name.name));
}

/*
 * Sample output:
 * fd 3 name ld.so.cache
 * fd 3 name libselinux.so.1
 */
```

### kfunc
* `kfunc[:module]:function`

Deprecated alias for `fentry`.

### kretfunc
* `kretfunc[:module:]function`

Deprecated alias for `fexit`.

### kprobe
**variants**
* `kprobe[:module]:fn`
* `kprobe[:module]:fn+offset`
**short names**
* `k`

``kprobe``s allow for dynamic instrumentation of kernel functions.
Each time the specified kernel function is executed the attached BPF programs are ran.

```
kprobe:tcp_reset {
  @tcp_resets = count()
}
```

Function arguments are available through the `argN` for register args. Arguments passed on stack are available using the stack pointer, e.g. `$stack_arg0 = **(int64**)reg("sp") + 16`.
Whether arguments passed on stack or in a register depends on the architecture and the number or arguments used, e.g. on x86_64 the first 6 non-floating point arguments are passed in registers and all following arguments are passed on the stack.
Note that floating point arguments are typically passed in special registers which don’t count as `argN` arguments which can cause confusion.
Consider a function with the following signature:

```
void func(int a, double d, int x)
```

Due to `d` being a floating point, `x` is accessed through `arg1` where one might expect `arg2`.

bpftrace does not detect the function signature so it is not aware of the argument count or their type.
It is up to the user to perform [Type conversion](#type-conversion) when needed, e.g.

```
#include <linux/path.h>
#include <linux/dcache.h>

kprobe:vfs_open
{
	printf("open path: %s\n", str(((struct path *)arg0).dentry.d_name.name));
}
```

Here arg0 was cast as a (struct path *), since that is the first argument to vfs_open.
The struct support is the same as bcc and based on available kernel headers.
This means that many, but not all, structs will be available, and you may need to manually define structs.

If the kernel has BTF (BPF Type Format) data, all kernel structs are always available without defining them. For example:

```
kprobe:vfs_open {
  printf("open path: %s\n", str(((struct path *)arg0).dentry.d_name.name));
}
```

You can optionally specify a kernel module, either to include BTF data from that module, or to specify that the traced function should come from that module.

```
kprobe:kvm:x86_emulate_insn
{
  $ctxt = (struct x86_emulate_ctxt *) arg0;
  printf("eip = 0x%lx\n", $ctxt.eip);
}
```

See [BTF Support](#btf-support) for more details.

`kprobe` s are not limited to function entry, they can be attached to any instruction in a function by specifying an offset from the start of the function.

### kretprobe
**variants**
* `kretprobe[:module]:fn`
**short names**
* `kr`

`kretprobe` s trigger on the return from a kernel function.
Return probes do not have access to the function (input) arguments, only to the return value (through `retval`).
A common pattern to work around this is by storing the arguments in a map on function entry and retrieving in the return probe:

```
kprobe:d_lookup
{
	$name = (struct qstr *)arg1;
	@fname[tid] = $name.name;
}

kretprobe:d_lookup
/@fname[tid]/
{
	printf("%-8d %-6d %-16s M %s\n", elapsed / 1e6, pid, comm,
	    str(@fname[tid]));
}
```

### profile

**variants**

* `profile:count`
* `profile:us:count`
* `profile:ms:count`
* `profile:s:count`
* `profile:hz:rate`

**short name**

* `p`

Profile probes fire on each CPU on the specified interval.
These operate using perf_events (a Linux kernel facility, which is also used by the perf command).
If a unit of time is not specified in the second position, the number is interpreted as nanoseconds; e.g., `interval:1s`, `interval:1000000000`, and `interval:s:1` are all equivalent.

```
profile:hz:99 { @[tid] = count(); }
```

### rawtracepoint

**variants**

* `rawtracepoint[:module]:event`

**short name**

* `rt`

Raw tracepoints are attached to the same tracepoints as normal tracepoint programs.
The reason why you might want to use raw tracepoints over normal tracepoints is due to the performance improvement - [Read More](https://docs.ebpf.io/linux/program-type/BPF_PROG_TYPE_RAW_TRACEPOINT/).

`rawtracepoint` arguments can be accessed via the `argN` builtins AND via the `args` builtin.

```
rawtracepoint:vmlinux:kfree_skb {
  printf("%llx %llx\n", arg0, args.skb);
}
```
`arg0` and `args.skb` will print the same address.

`rawtracepoint` probes make use of BTF type information to derive the type of function arguments at compile time.
This removes the need for manual type casting and makes the code more resilient against small signature changes in the kernel.
The arguments accessible by a `rawtracepoint` are different from the arguments you can access from the `tracepoint` of the same name.
The function arguments are available in the `args` struct which can be inspected by doing verbose listing (see [Listing Probes](../man/adoc/bpftrace.adoc#listing-probes)).

### software

**variants**

* `software:event:`
* `software:event:count`

**short name**

* `s`

These are the pre-defined software events provided by the Linux kernel, as commonly traced via the perf utility.
They are similar to tracepoints, but there is only about a dozen of these, and they are documented in the perf_event_open(2) man page.
If the count is not provided, a default is used.

The event names are:

* `cpu-clock` or `cpu`
* `task-clock`
* `page-faults` or `faults`
* `context-switches` or `cs`
* `cpu-migrations`
* `minor-faults`
* `major-faults`
* `alignment-faults`
* `emulation-faults`
* `dummy`
* `bpf-output`

```
software:faults:100 { @[comm] = count(); }
```

This roughly counts who is causing page faults, by sampling the process name for every one in one hundred faults.

### tracepoint

**variants**

* `tracepoint:subsys:event`

**short name**

* `t`

Tracepoints are hooks into events in the kernel.
Tracepoints are defined in the kernel source and compiled into the kernel binary which makes them a form of static tracing.
Unlike `kprobe` s, new tracepoints cannot be added without modifying the kernel.

The advantage of tracepoints is that they generally provide a more stable interface than `kprobe` s do, they do not depend on the existence of a kernel function.

```
tracepoint:syscalls:sys_enter_openat {
  printf("%s %s\n", comm, str(args.filename));
}
```

Tracepoint arguments are available in the `args` struct which can be inspected with verbose listing, see the [Listing Probes](../man/adoc/bpftrace.adoc#listing-probes) section for more details.

```
# bpftrace -lv "tracepoint:*"

tracepoint:xhci-hcd:xhci_setup_device_slot
  u32 info
  u32 info2
  u32 tt_info
  u32 state
...
```

Alternatively members for each tracepoint can be listed from their /format file in /sys.

Apart from the filename member, we can also print flags, mode, and more.
After the "common" members listed first, the members are specific to the tracepoint.

**Additional information**

* https://www.kernel.org/doc/html/latest/trace/tracepoints.html

### uprobe

**variants**
* `uprobe:binary:func`
* `uprobe:binary:func+offset`
* `uprobe:binary:offset`

**short names**
* `u`

`uprobe` s or user-space probes are the user-space equivalent of `kprobe` s.
The same limitations that apply [kprobe and kretprobe](#kprobe-and-kretprobe) also apply to `uprobe` s and `uretprobe` s, namely: arguments are available via the `argN` builtins and can only be accessed with a uprobe.
retval is the return value for the instrumented function and can only be accessed with a uretprobe.
**Note**: When tracing some languages, like C++, `arg0` and even `arg1` may refer to runtime internals such as the current object instance (`this`) and/or the eventual return value for large returned objects where copy elision is used.
This will push the actual function arguments to possibly start at `arg1` or `arg2` - the only way to know is to experiment.

```
uprobe:/bin/bash:readline { printf("arg0: %d\n", arg0); }
```

What does arg0 of readline() in /bin/bash contain?
I don’t know, so I’ll need to look at the bash source code to find out what its arguments are.

When tracing libraries, it is sufficient to specify the library name instead of
a full path. The path will be then automatically resolved using `/etc/ld.so.cache`:

```
uprobe:libc:malloc { printf("Allocated %d bytes\n", arg0); }
```

If multiple versions of the same shared library exist (e.g. `libssl.so.3` and
`libssl.so.59`), bpftrace may resolve the wrong one. To fix this, you can specify
a versioned SONAME to ensure the correct library is traced:

```
uprobe:libssl.so.3:SSL_write { ... }
```

If the traced binary has DWARF included, function arguments are available in the `args` struct which can be inspected with verbose listing, see the [Listing Probes](../man/adoc/bpftrace.adoc#listing-probes) section for more details.

```
# bpftrace -lv 'uprobe:/bin/bash:rl_set_prompt'

uprobe:/bin/bash:rl_set_prompt
    const char* prompt
```

When tracing C++ programs, it’s possible to turn on automatic symbol demangling by using the `:cpp` prefix:

```
# bpftrace:cpp:"bpftrace::BPFtrace::add_probe" { ... }
```

It is important to note that for `uretprobe` s to work the kernel runs a special helper on user-space function entry which overrides the return address on the stack.
This can cause issues with languages that have their own runtime like Golang:

**example.go**

```
func myprint(s string) {
  fmt.Printf("Input: %s\n", s)
}

func main() {
  ss := []string{"a", "b", "c"}
  for _, s := range ss {
    go myprint(s)
  }
  time.Sleep(1*time.Second)
}
```

**bpftrace**

```
# bpftrace -e 'uretprobe:./test:main.myprint { @=count(); }' -c ./test
runtime: unexpected return pc for main.myprint called from 0x7fffffffe000
stack: frame={sp:0xc00008cf60, fp:0xc00008cfd0} stack=[0xc00008c000,0xc00008d000)
fatal error: unknown caller pc
```

### uretprobe

**variants**
* `uretprobe:binary:func`

**short names**
* `ur`

### usdt

**variants**

* `usdt:binary_path:probe_name`
* `usdt:binary_path:[probe_namespace]:probe_name`
* `usdt:library_path:probe_name`
* `usdt:library_path:[probe_namespace]:probe_name`

**short name**

* `U`

Where probe_namespace is optional if probe_name is unique within the binary.

You can target the entire host (or an entire process’s address space by using the `-p` arg) by using a single wildcard in place of the binary_path/library_path:

```
usdt:*:loop { printf("hi\n"); }
```

Please note that if you use wildcards for the probe_name or probe_namespace and end up targeting multiple USDTs for the same probe you might get errors if you also utilize the USDT argument builtin (e.g. arg0) as they could be of different types.

Arguments are available via the `argN` builtins:

```
usdt:/root/tick:loop { printf("%s: %d\n", str(arg0), arg1); }
```

bpftrace also supports USDT semaphores.
If both your environment and bpftrace support uprobe refcounts, then USDT semaphores are automatically activated for all processes upon probe attachment (and --usdt-file-activation becomes a noop).
You can check if your system supports uprobe refcounts by running:

```
# bpftrace --info 2>&1 | grep "uprobe refcount"
bcc bpf_attach_uprobe refcount: yes
  uprobe refcount (depends on Build:bcc bpf_attach_uprobe refcount): yes
```

If your system does not support uprobe refcounts, you may activate semaphores by passing in -p $PID or --usdt-file-activation.
--usdt-file-activation looks through /proc to find processes that have your probe’s binary mapped with executable permissions into their address space and then tries to attach your probe.
Note that file activation occurs only once (during attach time).
In other words, if later during your tracing session a new process with your executable is spawned, your current tracing session will not activate the new process.
Also note that --usdt-file-activation matches based on file path.
This means that if bpftrace runs from the root host, things may not work as expected if there are processes execved from private mount namespaces or bind mounted directories.
One workaround is to run bpftrace inside the appropriate namespaces (i.e. the container).

### watchpoint
**variants**
* `watchpoint:absolute_address:length:mode`
* `watchpoint:function+argN:length:mode`

**short names**

* `w`

These are memory watchpoints provided by the kernel.
Whenever a memory address is written to (`w`), read
from (`r`), or executed (`x`), the kernel can generate an event.

### asyncwatchpoint

**variants**

* `asyncwatchpoint:absolute_address:length:mode`
* `asyncwatchpoint:function+argN:length:mode`

**short names**

* `aw`

This feature is experimental and may be subject to interface changes.
Memory watchpoints are also architecture dependent.

These are memory watchpoints provided by the kernel.
Whenever a memory address is written to (`w`), read
from (`r`), or executed (`x`), the kernel can generate an event.

In the first form, an absolute address is monitored.
If a pid (`-p`) or a command (`-c`) is provided, bpftrace takes the address as a userspace address and monitors the appropriate process.
If not, bpftrace takes the address as a kernel space address.

In the second form, the address present in `argN` when `function` is entered is
monitored.
A pid or command must be provided for this form.
If synchronous (`watchpoint`), a `SIGSTOP` is sent to the tracee upon function entry.
The tracee will be ``SIGCONT``ed after the watchpoint is attached.
This is to ensure events are not missed.
If you want to avoid the `SIGSTOP` + `SIGCONT` use `asyncwatchpoint`.

Note that on most architectures you may not monitor for execution while monitoring read or write.

```
# bpftrace -e 'watchpoint:0x10000000:8:rw { printf("hit!\n"); }' -c ./testprogs/watchpoint
```

Print the call stack every time the `jiffies` variable is updated:

```
watchpoint:0x$(awk '$3 == "jiffies" {print $1}' /proc/kallsyms):8:w {
  @[kstack] = count();
}
```

"hit" and exit when the memory pointed to by `arg1` of `increment` is written to:

```C
# cat wpfunc.c
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

__attribute__((noinline))
void increment(__attribute__((unused)) int _, int *i)
{
  (*i)++;
}

int main()
{
  int *i = malloc(sizeof(int));
  while (1)
  {
    increment(0, i);
    (*i)++;
    usleep(1000);
  }
}
```

```
# bpftrace -e 'watchpoint:increment+arg1:4:w { printf("hit!\n"); exit() }' -c ./wpfunc
```

Note that threads are monitored, but only for threads created after watchpoint attachment.
The is a limitation from the kernel.
Additionally, because of how watchpoints are implemented in bpftrace the specified function must be called at least once in the main thread in order to observe future calls to this function in child threads.

