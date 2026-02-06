# bpftrace-ls

[LSP](https://microsoft.github.io/language-server-protocol/) Languge Server for [bpftrace](https://github.com/bpftrace/bpftrace)

## Configuration

### bpftrace

The server internally runs `bpftrace`, either directly or via `sudo`, depending if run as root user or not .
If you run the server as ordinary user you need setup `sudo` to allow run  `bpftrace` without password.

For example open a custom sudoers file:
```bash
$ sudo visudo -f /etc/sudoers.d/bpftrace
```

And add below line:

```sudo
thisuser ALL=(root) NOPASSWD: /usr/bin/bpftrace
```
Replace user with your actual username and use correct path.
You can check those by below commands.

```bash
$ whoami
thisuser
$ which bpftrace
/usr/bin/bpftrace
```

### kernel

[BTF](https://docs.kernel.org/bpf/btf.html) (BPF Type Format) is higly utilized by `bpftrace-ls` . 
Is recommended to build your kernel with BTF support, by enabling below options:
```
CONFIG_DEBUG_INFO_BTF=y
CONFIG_DEBUG_INFO_BTF_MODULES=y
```
## Using in Neovim

### Filetype detection
Since Neovim 12 there is built-in filetype detection for `bpftrace`. If you are using older version
you can add the following to your `init.lua` file to detect `bpftrace` files:
```lua
vim.filetype.add({
  extension = {
    bt = "bpftrace"
  },
  pattern = {
    [".*"] = {
      function(path, bufnr)
        local first_line = vim.api.nvim_buf_get_lines(bufnr, 0, 1, false)[1] or ''
        if vim.regex([[^#!.*bpftrace]]):match_str(first_line) ~= nil then
          return "bpftrace"
        end
      end,
      { priority = -math.huge }
    }
  }
})
```
### Neovim LSP configuration
Once the filetype is recognized, you can register the language server.
Ensure that bpftrace-ls is in your $PATH or specify the full path.
See [documentation]( https://neovim.io/doc/user/lsp.html) for details.
```lua
-- LSP config for bpftrace-ls
vim.lsp.config['bpftrace-ls'] = {
  -- Command and arguments to start the server.
  cmd = { '/PATH/TO/bpftrace-ls/target/debug/bpftrace-ls' },
  filetypes = { 'bpftrace' },
}
-- Enable the server
vim.lsp.enable("bpftrace-ls")
```
