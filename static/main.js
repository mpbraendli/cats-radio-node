async function btn_send_packet() {

    let data = {
        'comment': null,
    };

    if (document.getElementById('with_comment').checked) {
        data.comment = document.getElementById('whisker_comment').value;
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
