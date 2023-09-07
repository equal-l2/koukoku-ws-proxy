# Koukoku WS Proxy

某公告 Telnet サーバ用の WebSocket プロクシです。

公告 →WS：放話のみを取り出して WebSocket に流します。  
WS→ 公告：受け取った Message をそのまま公告サーバに流します。

# 前提

Rust コンパイラが必要です。入れ方はググってください……

# ビルド＆実行

```sh
$ cargo run
```

`ws://localhost:8080/ws` に WS エンドポイントが立ちます。  
放話用の簡単な Web UI が `http://localhost:8080/ui` にあります。（Web UI 提供機能は feature flag でオフにできます）

# 使い道

1. Web クライアントのバックエンドとして使う
2. websocat などを使って CLI から叩く

# TODO

- 演説対応
- エスケープシーケンスの削除
