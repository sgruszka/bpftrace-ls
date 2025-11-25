# bpftrace-ls

bptftrace language server

## Testing in neovim
```bash
nvim --cmd ":luafile setup.lua" your_bpftrace_file.bt
```

## Sudo configuration
The server internally runs **sudo bpftrace**
 Therefore, you must allow your run bpftrace with sudo.
For example open a custom sudoers file:
```bash
$ sudo visudo -f /etc/sudoers.d/bpftrace
```

And add below line:

```sudo
user ALL=(root) NOPASSWD: /usr/bin/bpftrace
```
Replace user with your actual username and use correct path.
You can check the path using which command.

```bash
$ which bpftrace
/usr/bin/bpftrace
```

