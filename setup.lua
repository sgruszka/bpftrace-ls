vim.filetype.add({
  extension = {
    bt = "bpftrace"
  }
})

local client = vim.lsp.start_client({
  name = "bpftrace-ls",
  cmd = { "target/debug/bpftrace-ls" },

  -- on_attach = config.on_attach,
})

if not client then
  vim.notify("No LSP client")
  return
end


vim.api.nvim_create_autocmd("FileType", {
  pattern = "bpftrace",
  callback = function ()
    vim.lsp.buf_attach_client(0, client)
  end
})
