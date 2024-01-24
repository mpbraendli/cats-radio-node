
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
