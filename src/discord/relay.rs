use crate::chat_service::{FullMessage, Message};
use crate::{chat_service, CONFIG};
use anyhow::Result;
use reqwest;
use serde::Deserialize;
use serenity::http::Http;
use std::collections::HashMap;

use super::bot::{get_or_create_webhook_url, relayed_message_to_message, CONTEXT};

#[derive(Debug, Deserialize, Clone)]
struct WebhookResponse {
    id: String,
    channel_id: String,
}

fn sanitize(message: &str) -> String {
    const ZERO_WIDTH_SPACE: &str = "​";

    message.replace("@", &format!("@{ZERO_WIDTH_SPACE}"))
}

pub async fn delete_message(message: Message) {
    let relayed_messages = chat_service::message_relays(message.clone());
    let http = (*CONTEXT.lock()).as_ref().unwrap().http.clone();
    if relayed_messages.len() > 0 {
        for msg in relayed_messages {
            if msg.service == "discord" {
                let discord_msg = relayed_message_to_message(msg).await;
                if discord_msg.is_some() {
                    discord_msg.unwrap().delete(http.clone()).await;
                }
            }
        }
    }

    let origin_message = chat_service::message_origin(message.clone());
    if origin_message.is_some() && origin_message.clone().unwrap().clone().service == "discord" {
        let discord_msg = relayed_message_to_message(origin_message.unwrap()).await;
        if discord_msg.is_some() {
            discord_msg.unwrap().delete(http.clone()).await;
        }
    }
}

async fn send_message_webhook(
    webhook_url: String,
    message: String,
    username: Option<String>,
) -> WebhookResponse {
    let mut params = HashMap::new();
    params.insert("content", sanitize(&message));
    if username.is_some() {
        params.insert("username", username.unwrap());
    }

    println!("Sending message to {webhook_url}");

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}?wait=1", webhook_url))
        .form(&params)
        .send()
        .await
        .expect("Should have sent message!")
        .json::<WebhookResponse>()
        .await
        .expect("Should have parsed!");

    return res;
}

async fn edit_message_webhook(
    webhook: &str,
    message_id: String,
    message: String,
) -> WebhookResponse {
    let mut params = HashMap::new();
    params.insert("content", sanitize(&message));

    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{}/messages/{}", webhook, message_id))
        .form(&params)
        .send()
        .await
        .expect("Should have sent message!")
        .json::<WebhookResponse>()
        .await
        .expect("Should have parsed!");

    return res;
}

pub async fn relay_message(http: &Http, message: FullMessage) -> Result<Message> {
    let room = CONFIG
        .room
        .iter()
        .find(|room| room.matrix == message.message.room_id);
    let Some(room) = room else{
        return Ok(message.message);
    };

    let webhook_url = get_or_create_webhook_url(http, room.discord).await?;

    let wh = send_message_webhook(
        webhook_url,
        message.content,
        Some(format!("{} ({})", message.user.display, message.user.tag).to_owned()),
    )
    .await;

    Ok(Message {
        service: "discord".to_owned(),
        server_id: room.discord_guild.to_string(),
        room_id: room.discord.to_string(),
        id: wh.id,
    })
}

pub async fn edit_message(http: &Http, message: FullMessage) {
    let room = CONFIG
        .room
        .iter()
        .find(|room| room.matrix == message.message.room_id);

    let Some(room) = room else {
        return;
    };

    let webhook_url = match get_or_create_webhook_url(http, room.discord).await {
        Ok(w) => w,
        Err(err) => {
            println!("Error getting webhook: {}", err);
            return;
        }
    };

    let relayed_messages = chat_service::message_relays(message.clone().message);
    for msg in relayed_messages {
        if msg.service == "discord" {
            edit_message_webhook(&webhook_url, msg.id, message.clone().content).await;
        }
    }
}
