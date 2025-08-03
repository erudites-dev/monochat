use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use futures::{SinkExt, StreamExt};
use reqwest::{Client, IntoUrl, header::CONTENT_TYPE};
use serde::Deserialize;
use tokio::{spawn, time::sleep};
use tokio_tungstenite::tungstenite::{self, ClientRequestBuilder};

use crate::{Message, MessageStream};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct AquaApi {
    channel: AquaApiChannel,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct AquaApiChannel {
    chdomain: String,
    chpt: String,
    chatno: u32,
    pwd: String,
}

const HEADER_LEN: usize = 2 + 4 + 6 + 2;

fn write_packet(packet_type: u32, body: impl AsRef<str>) -> String {
    format!(
        "\x1B\x09{:04}{:06}{:02}{}",
        packet_type,
        body.as_ref().len(),
        0,
        body.as_ref()
    )
}

fn write_keepalive() -> String {
    write_packet(0x00, "\x0C")
}

fn write_login() -> String {
    write_packet(0x01, "\x0C\x0C\x0C16\x0C")
}

fn write_joinch(chatno: u32, pwd: String) -> String {
    write_packet(
        0x02,
        format!("\x0C{chatno}\x0C\x0C0\x0C\x0Clog\x11\x12pwd\x11{pwd}\x12\x0C"),
    )
}

pub fn parse_packet(packet: &str) -> Result<(u32, &str)> {
    if packet.len() < HEADER_LEN {
        return Err(anyhow!("packet too short"));
    }
    let packet_type = packet[2..6]
        .parse::<u32>()
        .map_err(|_| anyhow!("failed to parse packet type"))?;
    Ok((packet_type, &packet[HEADER_LEN..]))
}

pub fn handle_chatmesg(body: &str) -> Result<Message> {
    let mut body = body.split("\x0C");
    let message = body
        .nth(1)
        .ok_or_else(|| anyhow!("invalid chatmesg packet"))?;
    let name = body
        .nth(4)
        .ok_or_else(|| anyhow!("invalid chatmesg packet"))?;
    Ok(Message {
        sender: name.to_string(),
        content: Some(message.to_string()),
        donated: None,
    })
}

pub fn handle_sendballoon(body: &str) -> Result<Message> {
    let mut body = body.split("\x0C");
    let user = body
        .nth(2)
        .ok_or_else(|| anyhow!("invalid sendballoon packet"))?;
    let amount = body
        .nth(1)
        .ok_or_else(|| anyhow!("invalid sendballoon packet"))?;
    Ok(Message {
        sender: user.to_string(),
        content: None,
        donated: Some(
            amount
                .parse::<u64>()
                .map_err(|_| anyhow!("failed to parse amount"))?,
        ),
    })
}

fn handle_setbjstat(is_alive: &Arc<AtomicBool>) -> Result<Message> {
    is_alive.store(false, Ordering::Relaxed);
    Err(anyhow!("continue"))
}

fn handle_message(message: tungstenite::Message, is_alive: &Arc<AtomicBool>) -> Result<Message> {
    let text = message
        .into_text()
        .map_err(|_| anyhow!("failed to convert message to text"))?;
    let (packet_type, body) = parse_packet(&text)?;
    match packet_type {
        0x05 => handle_chatmesg(body),
        0x07 => handle_setbjstat(is_alive),
        0x12 => handle_sendballoon(body),
        _ => Err(anyhow!("continue")),
    }
}

/// https://aqua.sooplive.co.kr/component.php?szKey=<key>
pub async fn new(aqua_url: impl IntoUrl) -> Result<impl MessageStream> {
    let aqua_url = aqua_url.into_url().map_err(|_| anyhow!("invalid url"))?;
    let aqua_url_query = aqua_url
        .query()
        .ok_or_else(|| anyhow!("invalid aqua url"))?;
    let response = Client::new()
        .post("https://live.sooplive.co.kr/api/aqua_api.php")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(aqua_url_query.to_string())
        .send()
        .await
        .map_err(|_| anyhow!("failed to send request"))?;
    let AquaApi { channel } = response
        .json::<AquaApi>()
        .await
        .map_err(|_| anyhow!("failed to parse response"))?;
    let AquaApiChannel {
        chdomain,
        chpt,
        chatno,
        pwd,
    } = channel;
    let url = format!("ws://{}:{}/Websocket", chdomain, chpt);
    let request =
        ClientRequestBuilder::new(url.parse().map_err(|_| anyhow!("invalid websocket url"))?)
            .with_sub_protocol("chat");
    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|_| anyhow!("failed to connect to websocket"))?;
    socket
        .send(write_login().into())
        .await
        .map_err(|_| anyhow!("failed to send login packet"))?;
    socket
        .send(write_joinch(chatno, pwd).into())
        .await
        .map_err(|_| anyhow!("failed to send join packet"))?;
    socket.next().await;
    let (mut sender, receiver) = socket.split();
    let is_alive = Arc::new(AtomicBool::new(true));
    let is_alive_for_stream = is_alive.clone();
    let message_stream = receiver
        .filter_map(move |result| {
            let is_alive_clone = is_alive_for_stream.clone();
            async move {
                match result {
                    Ok(message) => match handle_message(message, &is_alive_clone) {
                        Ok(message) => Some(message),
                        Err(_) => None,
                    },
                    Err(_) => None,
                }
            }
        })
        .take_until(spawn(async move {
            while is_alive.load(Ordering::Relaxed) {
                sleep(Duration::from_secs(6)).await;
                if sender.send(write_keepalive().into()).await.is_err() {
                    break;
                }
            }
        }))
        .boxed();
    Ok(message_stream)
}
