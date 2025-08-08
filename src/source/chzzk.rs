use std::fmt::Debug;

use anyhow::{Error, Result, anyhow};
use futures::{Sink, SinkExt, StreamExt, stream};
use reqwest::{Client, IntoUrl};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::value::RawValue;
use tokio::time::{Duration, sleep};
use tokio_tungstenite::tungstenite::Message;

use crate::MessageStream;

#[derive(Debug, Deserialize)]
struct Response<T: Debug> {
    code: i64,
    message: Option<String>,
    content: T,
}

async fn get<T: DeserializeOwned + Debug>(url: impl IntoUrl, params: &[(&str, &str)]) -> Result<T> {
    let json = Client::new()
        .get(url)
        .query(params)
        .send()
        .await?
        .json::<Response<T>>()
        .await?;
    if json.code != 200 {
        if let Some(message) = json.message {
            return Err(anyhow!("chzzk api error: {} ({})", json.code, message));
        } else {
            return Err(anyhow!("chzzk api error: {}", json.code));
        }
    }
    Ok(json.content)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatChannelId {
    chat_channel_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessToken {
    access_token: String,
}

#[derive(Debug, Serialize)]
struct SendCommand<'a> {
    #[serde(rename = "svcid")]
    service_id: &'a str,
    #[serde(rename = "cid")]
    channel_id: &'a str,
    #[serde(rename = "cmd")]
    command: i64,
    #[serde(rename = "ver")]
    version: i64,
    #[serde(rename = "bdy")]
    body: &'a RawValue,
}

#[derive(Debug, Deserialize)]
struct RecvCommand<'a> {
    #[serde(rename = "bdy", borrow)]
    body: &'a RawValue,
}

trait SendCommandBody: Serialize + Sized {
    fn command(&self) -> i64;

    fn version(&self) -> i64;

    async fn send(
        self,
        channel_id: &str,
        mut socket: impl Sink<Message, Error = impl Into<Error>> + Unpin,
    ) -> Result<()> {
        let command = SendCommand {
            service_id: "game",
            channel_id,
            command: self.command(),
            version: self.version(),
            body: &RawValue::from_string(serde_json::to_string(&self)?)?,
        };
        let message = Message::text(serde_json::to_string(&command)?);
        socket.send(message).await.map_err(Into::into)?;
        Ok(())
    }
}

trait RecvCommandBody<'a>: Deserialize<'a> {
    fn recv(message: &'a Message) -> Result<Self> {
        let command = serde_json::from_str::<RecvCommand>(message.to_text()?)?;
        Ok(serde_json::from_str(command.body.get())?)
    }
}

#[derive(Debug, Serialize)]
struct Auth {
    #[serde(rename = "accTkn")]
    access_token: String,
    auth: &'static str,
}

impl Auth {
    fn new(access_token: String) -> Self {
        Self {
            access_token,
            auth: "READ",
        }
    }
}

impl SendCommandBody for Auth {
    fn command(&self) -> i64 {
        100
    }

    fn version(&self) -> i64 {
        3
    }
}

#[derive(Debug, Serialize)]
struct KeepAlivePing;

impl SendCommandBody for KeepAlivePing {
    fn command(&self) -> i64 {
        0
    }

    fn version(&self) -> i64 {
        3
    }
}
#[derive(Debug, Serialize)]
struct KeepAlivePong;

impl SendCommandBody for KeepAlivePong {
    fn command(&self) -> i64 {
        10000
    }

    fn version(&self) -> i64 {
        3
    }
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(rename = "profile", deserialize_with = "deserialize_sender")]
    sender: String,
    #[serde(rename = "msg")]
    content: String,
    #[serde(rename = "extras", deserialize_with = "deserialize_donated")]
    donated: Option<u64>,
}

fn deserialize_sender<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    #[derive(Debug, Deserialize)]
    struct Profile {
        nickname: String,
    }
    let profile = String::deserialize(deserializer).map_err(serde::de::Error::custom)?;
    let profile = serde_json::from_str::<Profile>(&profile).map_err(serde::de::Error::custom)?;
    Ok(profile.nickname)
}

fn deserialize_donated<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    #[derive(Debug, Deserialize)]
    struct Extras {
        pay_amount: Option<u64>,
    }
    let extras = String::deserialize(deserializer)?;
    let extras = serde_json::from_str::<Extras>(&extras).map_err(serde::de::Error::custom)?;
    Ok(extras.pay_amount)
}

type ChatCommand = Vec<ChatMessage>;

impl<'a> RecvCommandBody<'a> for Vec<ChatMessage> {}

/// You can use either one of:
/// \
/// chat in the form of `https://api.chzzk.naver.com/manage/v1/chats/sources/<uuid>`,
/// \
/// live in the form of `https://api.chzzk.naver.com/polling/v3.1/channels/<uuid>/live-status`
pub async fn new(url: impl IntoUrl) -> Result<impl MessageStream> {
    let ChatChannelId {
        chat_channel_id: channel_id,
    } = get::<ChatChannelId>(url, &[]).await?;
    let AccessToken { access_token } = get::<AccessToken>(
        "https://comm-api.game.naver.com/nng_main/v1/chats/access-token",
        &[("channelId", &channel_id), ("chatType", "STREAMING")],
    )
    .await?;
    let (socket, _) = tokio_tungstenite::connect_async("wss://kr-ss1.chat.naver.com/chat").await?;
    let (mut send, recv) = socket.split();
    Auth::new(access_token).send(&channel_id, &mut send).await?;
    let stream = recv
        .filter_map(|message| async {
            let Ok(message) = message else {
                return Some(stream::iter(Vec::new()));
            };
            let Ok(command) = ChatCommand::recv(&message) else {
                return Some(stream::iter(Vec::new()));
            };
            Some(stream::iter(command))
        })
        .flatten()
        .map(|message| crate::Message {
            sender: message.sender,
            content: Some(message.content),
            donated: message.donated,
        })
        .take_until(async move {
            loop {
                sleep(Duration::from_secs(30)).await;
                // this is wacky, but it theoretically should work :)
                if KeepAlivePing
                    .send(channel_id.as_str(), &mut send)
                    .await
                    .is_err()
                {
                    break;
                }
                if KeepAlivePong
                    .send(channel_id.as_str(), &mut send)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        })
        .boxed();
    Ok(stream)
}
