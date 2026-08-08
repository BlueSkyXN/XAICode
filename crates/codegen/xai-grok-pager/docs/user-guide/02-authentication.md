# Authentication

XAICode does not provide browser login, account OAuth/OIDC, device-code flow,
cached account sessions, or billing authentication. The active authentication
surface is a normal API key for the OpenAI-compatible endpoint selected by the
model or endpoint configuration.

## Environment setup

```sh
export CODING_AGENT_API_BASE_URL=http://127.0.0.1:8000/v1
export CODING_AGENT_API_KEY=your-key
xaicode
```

`OPENAI_API_KEY` is accepted as a compatibility fallback. A model entry may
instead set its own `base_url`, `api_key`, or ordered `env_key`; those
credentials stay in the process-local provider path and are not written to an
account file.

```toml
[model.local]
model = "my-coder"
base_url = "https://inference.example/v1"
env_key = ["MY_CODER_API_KEY", "OPENAI_API_KEY"]
api_backend = "responses"
```

Only endpoints explicitly configured by the user are eligible. Vendor-hosted
XAI/XAICode domains and credentials are rejected by the clean-build endpoint
sanitizer before request I/O.

## ACP clients

When the agent is started with `xaicode agent stdio`, the ACP server advertises
one generic `api_key` method when a configured key is available. There is no
interactive login screen. If no key is available, configure the provider before
starting the session; the local TUI remains usable for local inspection and
configuration. `xai-grok-pager` is the compatibility command for clients that
still invoke the upstream pager binary.

## Credential hygiene

- Do not put API keys in project files that will be committed.
- Prefer environment variables or a user-owned secret manager.
- The clean build does not read legacy `auth.json`, `GROK_AUTH`, or OIDC
  refresh-token files for account authentication.
- `/login`, `/logout`, account switching, and device-auth commands are absent
  from the public CLI. Stale ACP requests return a local-build-disabled error.
- Third-party MCP OAuth remains available only for explicitly configured MCP
  servers; it never supplies credentials for the inference provider.

## Troubleshooting

Use `xaicode doctor` for local terminal, filesystem, sandbox, and MCP checks.
For a provider error, verify `CODING_AGENT_API_BASE_URL`, the selected model's
`base_url`, and the corresponding API-key environment variable. A vendor URL or
an attempt to use the removed hosted account service is rejected before any
network request is made.
