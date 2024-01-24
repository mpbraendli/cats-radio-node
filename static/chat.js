var socket = null;
var retry_scheduled = false;

function init_socket() {

    retry_scheduled = false;

    if (socket !== null) {
        socket.onmessage = null;
        socket.onopen = null;
        socket.onclose = null;
        socket.onerror = null;

        delete socket;
    }

    socket = new WebSocket("ws://" + window.location.host + "/chat/ws");

    socket.onmessage = function(data) {
        const message = JSON.parse(data.data)
        add_message(message);
    }

    socket.onopen = function(_data) {
        console.log("Websocket open");
    }

    socket.onclose = function(_code, text) {
        if (!retry_scheduled) {
            console.log(`Websocket closed ${text}`);
            retry_scheduled = true;
            init_socket();
        }
    }

    socket.onerror = function(e) {
        if (!retry_scheduled) {
            console.log(`Websocket error because ${e}. Trying again in 3s`);
            retry_scheduled = true;
            setTimeout(init_socket, 3000);
        }
    }
}

function keep_alive() {
    if (socket !== null && socket.readyState == 1) {
        try {
            socket.send('{}');
        } catch (e) {
        }
    }

    setTimeout(keep_alive, 10000);
}

function add_message(message) {
    let template = document.getElementById('message_template');
    let clon = template.content.cloneNode(true);

    const msg_timestamp = clon.querySelector("div.msg_timestamp");
    const ts = strftime("%Y-%m-%d %H:%M:%S", new Date(message.received_at * 1000));
    msg_timestamp.textContent = `${ts} UTC`;

    const msg_from = clon.querySelector("div.msg_from");
    msg_from.textContent = `${message.from_callsign}-${message.from_ssid}`;

    const msg_comment = clon.querySelector("div.msg_comment");
    msg_comment.textContent = message.comment;

    const messagelist = document.getElementById('messagelist');
    messagelist.appendChild(clon);
    messagelist.scrollTo(0, messagelist.scrollHeight);
}

function call_clicked(element_clicked) {
    let call_ssid = element_clicked.textContent;
    document.getElementById('dest').value = call_ssid;
}

async function btn_chat_send_message() {
    let data = {
        'comment': document.getElementById('whisker_comment').value,
        'destinations': [],
    };

    let callsign_ssid = document.getElementById('dest').value;
    if (callsign_ssid !== "") {
        let splitted = callsign_ssid.split("-");
        let ssid = 0;
        if (splitted.length == 2) {
            ssid = parseInt(splitted[1], 10);
        }
        data.destinations.push({'callsign': splitted[0], 'ssid': ssid});
    }
    await post('/api/send_packet', data);
}

window.addEventListener("load", (_event) => {
    init_socket();
    keep_alive();
});

