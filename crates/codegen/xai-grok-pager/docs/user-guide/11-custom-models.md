# Custom Models

XAICode uses OpenAI-compatible model entries. The clean build has no
hosted model catalog; the configured local/provider endpoint is the source of
truth.

## Select a model

```sh
xaicode models
xaicode -p "Explain this code" -m local-model
```

The bundled default is `local-model`. Set a different default in the user
configuration directory selected by `GROK_HOME` (the variable is retained for
upstream path compatibility):

```toml
[models]
default = "my-coder"

[model.my-coder]
model = "my-coder"
base_url = "https://inference.example/v1"
env_key = "MY_CODER_API_KEY"
api_backend = "responses"
context_window = 128000
```

`api_backend` accepts the protocol supported by the provider (`responses`,
`chat_completions`, or `messages`). `extra_headers` and `env_http_headers` are
available for provider-specific, user-supplied headers. The clean sampler
filters the removed vendor marker headers before a request is built.

## Endpoint and credentials

The active endpoint may be supplied by:

1. a model entry's `base_url`;
2. `CODING_AGENT_API_BASE_URL`; or
3. the compatibility `OPENAI_BASE_URL` environment variable.

Credentials may be supplied by a model's `api_key`/`env_key`,
`CODING_AGENT_API_KEY`, or `OPENAI_API_KEY`. Browser login, cached account
tokens, and the former hosted API endpoint are not fallback sources.

```sh
export CODING_AGENT_API_BASE_URL=https://inference.example/v1
export CODING_AGENT_API_KEY="$MY_CODER_KEY"
xaicode
```

Vendor-owned XAI/XAICode hosts are rejected by the endpoint sanitizer, including
values supplied through stale configuration files or environment variables.

## Auxiliary model pins

The same model-entry format can be used for auxiliary tasks:

```toml
[models]
web_search = "my-coder"
session_summary = "my-coder"
image_description = "my-coder"
```

The built-in web-search/media integrations are disabled in the clean runtime;
these fields only remain for configuration compatibility and local providers.

## Troubleshooting

Use `xaicode inspect --json` to inspect the resolved local model entries.
If a request fails, check the endpoint path (usually `/v1`), the selected
backend, and the environment variable named by `env_key`. A removed vendor
host fails before network I/O with a clean-build-disabled error.
