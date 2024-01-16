async function btn_add_destination() {
    const template = document.getElementById('destination_template');

    let clon = template.content.cloneNode(true);
    document.getElementById('destinations').appendChild(clon);
}

async function btn_remove_destination(element_clicked) {
    element_clicked.parentElement.remove()
}

async function btn_send_packet() {
    let data = {
        'comment': null,
        'destinations': [],
    };

    if (document.getElementById('with_comment').checked) {
        data.comment = document.getElementById('whisker_comment').value;
    }

    const destinations = document.getElementById('destinations');
    const destList = destinations.querySelectorAll("p.destination");
    for (let i = 0; i < destList.length; i++) {
        const dest_callsign = destList[i].querySelector("input.dest_callsign").value;
        const dest_ssid_str = destList[i].querySelector("input.dest_ssid").value;
        const dest_ssid = parseInt(dest_ssid_str, 10);
        if (dest_ssid < 0 || dest_ssid > 255) {
            alert("SSID must be between 0 and 255");
            return;
        }
        data.destinations.push({'callsign': dest_callsign, 'ssid': dest_ssid});
    }

    await post('/api/send_packet', data);
}

async function post(url, data) {
    const params = {
        method: "POST",
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify(data),
    };

    let response = await fetch(url, params);
    if (!response.ok) {
        const text = await response.text();
        alert(`Error Sending: ${response.statusText} ${text}`);
    }
}
