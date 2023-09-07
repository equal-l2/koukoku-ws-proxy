/** @type HTMLTextAreaElement */
const msgView = document.getElementById("msg-view");
msgView.textContent = ""; // HTMLでミスったとき用のフールプルーフ

/** @type HTMLInputElement */
const msgInput = document.getElementById("msg-input");

/** @type HTMLButtonElement */
const sendButton = document.getElementById("send-button");

/** @type HTMLButtonElement */
const connectButton = document.getElementById("connect-button");

// https://www.w3schools.com/howto/howto_js_trigger_button_enter.asp
msgInput.addEventListener("keypress", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    document.getElementById("send-button").click();
  }
});

/**
 * @param {string} s
 */
function validate(s) {
  if (s === "") {
    return false;
  }
  return true; // TODO
}

function connect() {
  const sock = new WebSocket("ws://localhost:8080/ws");
  sock.onopen = () => {
    console.log("WS connected");
    function recv(ev) {
      // assume the message is in text type
      msgView.textContent += ev.data + "\n";
      msgView.scrollTop = msgView.scrollHeight;
    }
    sock.onmessage = recv;

    function send() {
      console.log("send");
      const msg = msgInput.value;
      if (validate(msg)) {
        sock.send(msg + "\n");
        msgInput.value = "";
      } else {
        alert("バリデーション失敗"); // TODO: better UI
      }
    }
    sendButton.onclick = send;

    sendButton.disabled = false;
    connectButton.disabled = true;
  };

  sock.onclose = () => {
    console.log("WS disconnected");
    alert("WebSocketが切断されました");
    sendButton.disabled = true;
    connectButton.disabled = false;
  };
}
connect();

connectButton.onclick = connect;
