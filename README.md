# mcapi-rs

Rewrite of [mcapi] in Rust.

[mcapi]: https://github.com/Syfaro/mcapi

## Configuration

| Name           | Description                                                                                                        |
| -------------- | ------------------------------------------------------------------------------------------------------------------ |
| `HTTP_HOST`    | Host to listen for incoming HTTP requests, defaults to `0.0.0.0:8080`                                              |
| `REDIS_SERVER` | Redis server to use for caching server information and locking, should be formatted like `redis://127.0.0.1:6379/` |
