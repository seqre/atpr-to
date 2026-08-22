# Deployment

**Prerequisites:** AWS SAM CLI, [cargo-lambda](https://www.cargo-lambda.info/), ARM64 cross-compilation target.

```sh
just deploy        # guided (first time)
just deploy-fast   # subsequent deploys (uses samconfig.toml from the guided run)
just logs          # tail Lambda logs
```

The SAM template deploys a single `provided.al2023` Lambda function on arm64 behind an HTTP API Gateway,
with throttling set at the gateway — the in-process rate limiter is per execution environment and cannot
bound anything globally.

A custom domain is optional. Pass `DomainName` **and** a regional `CertificateArn` to have the stack create
the domain and its mapping; omit both and it serves from the `execute-api` URL in the stack outputs.

OAuth sessions live in the DynamoDB table the stack creates, shared by every execution environment. They
used to be written to the instance's `/tmp`, so login state saved while handling the redirect could be
missing when the callback landed on a different instance and logins failed intermittently under any
concurrency. `session_file` selects the backend: `dynamodb://{table}` for the deployed stack, a path for a
file, and `""` for memory.
