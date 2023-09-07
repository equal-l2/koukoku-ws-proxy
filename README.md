# Koukoku WS Proxy

某公告 Telnet サーバ用の WebSocket プロクシです。  
デフォルトでは `ws://localhost:8080/ws` に WS エンドポイントを立てます。

公告 →WS：放話のみを取り出して WebSocket に流します。  
WS→ 公告：受け取った Message をそのまま公告サーバに流します。

# 使い道

1. Web クライアントのバックエンドとして使う
2. websocat などを使って CLI から叩く

# TODO

- 演説対応
- エスケープシーケンスの削除
