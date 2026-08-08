# `xai-grok-mcp` navigation card

This crate implements user-configured MCP transports, server discovery and third-party OAuth.
Read this card before changing OAuth, browser consent, token refresh/storage, credentials,
transport startup, or server configuration.

## Local invariants

- MCP OAuth is not Grok/xAI account login. Preserve standards-based consent for explicitly
  configured MCP servers, including refresh and non-interactive handling.
- OAuth URLs, client credentials and scopes come from the configured/discovered MCP server;
  never fall back to xAI account issuers, cached Grok sessions, or product auth files.
- Browser opening is allowed only as a direct consequence of the user's configured MCP server
  requiring consent. Headless/non-interactive modes must not hang waiting for a browser flow.
- Preserve ordinary Authorization headers and bring-your-own OAuth client configuration.
- MCP credentials remain separate from xAI account `auth.json`; concurrent read-modify-write
  must not roll back a newly refreshed token.
- Do not auto-register an xAI/official hosted MCP catalog in the clean composition root.

## Do not

- Do not remove generic MCP OAuth merely to satisfy the no-xAI-login rule.
- Do not log access tokens, refresh tokens, client secrets or complete Authorization headers.
- Do not test against live credentials; use local fixtures and temporary credential stores.

## Validation

Use focused `xai-grok-mcp` tests when its dependency closure is available, then run the root
`xaicode` package checks. Cover refresh success, terminal versus transient refresh failure,
non-interactive behavior, deduplicated browser flow and concurrent credential persistence.
