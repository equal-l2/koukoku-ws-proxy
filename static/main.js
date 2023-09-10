/** @type HTMLTextAreaElement */
const houwaView = document.getElementById("houwa-view");

/** @type HTMLTextAreaElement */
const enzetsuView = document.getElementById("enzetsu-view");

/** @type HTMLInputElement */
const houwaInput = document.getElementById("houwa-input");

/** @type HTMLButtonElement */
const sendButton = document.getElementById("send-button");

/** @type HTMLButtonElement */
const connectButton = document.getElementById("connect-button");

// https://www.w3schools.com/howto/howto_js_trigger_button_enter.asp
houwaInput.addEventListener("keypress", (event) => {
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
      // handle only if the message is in text type
      if (ev.data != null && typeof ev.data === "string") {
        msg = JSON.parse(ev.data);
        switch(msg.kind) {
          case "Houwa": 
            houwaView.textContent += msg.payload + "\n";
            houwaView.scrollTop = houwaView.scrollHeight;
            break;
          case "Enzetsu": 
          // 演説は無加工で送っているので改行等はそのまま使える
          enzetsuView.textContent += msg.payload;
          enzetsuView.scrollTop = enzetsuView.scrollHeight;
          break;
          default:
            console.warn(`Unknown payload kind: ${msg.kind}`)
            break;
        }
      } else {
        console.warn(`Malformed message sent: ${ev.data}`)
      }
    }
    sock.onmessage = recv;

    function send() {
      console.log("send");
      const houwa = houwaInput.value;
      if (validate(houwa)) {
        sock.send(houwa + "\n");
        houwaInput.value = "";
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
