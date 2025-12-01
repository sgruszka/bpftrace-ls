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

local client = vim.lsp.start_client({
  name = "bpftrace-ls",
  cmd = { "target/debug/bpftrace-ls" },
  -- on_attach = config.on_attach,
})

vim.api.nvim_create_autocmd("FileType", {
  pattern = "bpftrace",
  callback = function ()
    vim.bo.commentstring = "// %s"
    if client then
      vim.lsp.buf_attach_client(0, client)
    end
  end
})
