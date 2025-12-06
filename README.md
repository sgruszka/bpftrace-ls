# bpftrace-ls

[LSP](https://microsoft.github.io/language-server-protocol/) Languge Server for [bpftrace](https://github.com/bpftrace/bpftrace)

## Sudo configuration
The server internally runs **sudo bpftrace**
Therefore, you must allow to run bpftrace with sudo.
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
## Using in Neovim

### Filetype detection
Vim/Neovim does not currently provide built-in filetype detection for `bpftrace`.
To enable automatic detection of `bpftrace` files, based on the `*.bt` extension or a proper shebang, you can add the following to your `init.lua`:
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
