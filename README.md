# Koukoku WS Proxy
某公告Telnetサーバ用のWebSocketプロクシです。  
デフォルトでは `ws://localhost:8080/ws` にWSエンドポイントを立てます。  

公告→WS：放話のみを取り出してWebSocketに流します。  
WS→公告：受け取ったMessageをそのまま公告サーバに流します。  

# 使い道
1. Webクライアントのバックエンドとして使う
2. websocatなどを使ってCLIから叩く