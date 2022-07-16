# mcapi-rs

Rewrite of [mcapi] in Rust.

原始仓库：[mcapi-rs]

[mcapi]: https://github.com/Syfaro/mcapi
[mcapi-rs]: https://github.com/Syfaro/mcapi-rs

## 配置

| Name           | Description                                                                                                        |
| -------------- | ------------------------------------------------------------------------------------------------------------------ |
| `HTTP_HOST`    | Host to listen for incoming HTTP requests, defaults to `0.0.0.0:8080`                                              |
| `REDIS_SERVER` | Redis server to use for caching server information and locking, should be formatted like `redis://127.0.0.1:6379/` |

如需对配置项进行更改，可直接在命令台设置对应配置项的环境变量。  
**特别注意：`REDIS_SERVER`项无默认配置，请按描述进行配置。**

## 运行方法

### 编译

```
cargo build --release
```

### 运行

```
cargo run --release
```

### 测试

```
cargo test
```

## 仓库特别描述

为显示中文，本仓库中的图片字体已更改为`微软雅黑`，然而仓库内并未上传该字体。  
如需在本地搭建，请首先导入`微软雅黑`字体库或手动更换为其他字体。
