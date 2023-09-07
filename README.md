# Koukoku WS Proxy

某公告 Telnet サーバ用の WebSocket プロクシです。  

公告 →WS：放話のみを取り出して WebSocket に流します。  
WS→ 公告：受け取った Message をそのまま公告サーバに流します。

# 前提
Rustコンパイラが必要です。入れ方はググってください……

# ビルド＆実行
デフォルトでは `ws://localhost:8080/ws` に WS エンドポイントを立てます。
```sh
$ cargo run
```

# 使い道

1. Web クライアントのバックエンドとして使う
2. websocat などを使って CLI から叩く

# TODO

- 演説対応
- エスケープシーケンスの削除

# おまけ
プロキシを使った簡単なWeb UIが`example-ui`に入っています。miniserveとか使ってサーブして使ってください。