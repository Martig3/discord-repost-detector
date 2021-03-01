use std::env;

use chrono::{DateTime, Utc};
use image;
use img_hash::{HasherConfig, ImageHash};
use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::StandardFramework;
use serenity::model::channel::{Attachment, Message};
use serenity::model::user::User;
use serenity::prelude::TypeMapKey;
use serenity::utils::{MessageBuilder};

struct Handler;

struct ImageMetadata {
    hash: ImageHash,
    timestamp: DateTime<Utc>,
    user: User,
    msg_link: String,
}

struct Metadatacache;

struct Config {
    cache_limit: u64,
}

impl TypeMapKey for Metadatacache {
    type Value = Vec<ImageMetadata>;
}

impl TypeMapKey for Config {
    type Value = Config;
}

#[tokio::main]
async fn main() {
    let framework = StandardFramework::new();
    // Login with a bot token from the environment
    let token = env::var("REPOST_DISCORD_TOKEN").expect("Expected a REPOST_DISCORD_TOKEN env var");
    let mut client = Client::builder(token)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");
    {
        let mut data = client.data.write().await;
        data.insert::<Metadatacache>(Vec::new());
        data.insert::<Config>(read_config());
    }
    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

fn read_config() -> Config {
    let cache_limit = env::var("REPOST_CACHE_LIMIT").expect("Expected a REPOST_CACHE_LIMIT env var").parse::<u64>().unwrap();
    let config = Config { cache_limit };
    config
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, context: Context, msg: Message) {
        let attachments: Vec<&Attachment> = msg.attachments.iter().map(|a| a).collect();
        let embedded: Vec<String> = msg.embeds.iter()
            .map(|e| e.url.clone())
            .filter(|e| e.is_some())
            .map(|e| e.unwrap().clone())
            .collect();
        if attachments.is_empty() && embedded.is_empty() {
            return;
        }
        let mut hashes: Vec<ImageHash> = Vec::new();
        for url in embedded {
            if let Ok(resp) = reqwest::get(&url).await {
                if let Ok(img_bytes) = resp.bytes().await {
                    let image_hash = HasherConfig::new().to_hasher().hash_image(&image::load_from_memory(img_bytes.as_ref()).unwrap());
                    hashes.push(image_hash);
                }
            }
        }
        for attachment in attachments {
            if let Ok(img) = attachment.download().await {
                let image_hash = HasherConfig::new().to_hasher().hash_image(&image::load_from_memory(img.as_ref()).unwrap());
                hashes.push(image_hash);
            }
        }
        let mut data = context.data.write().await;
        let config: &Config = data.get::<Config>().unwrap().clone();
        let cache_size = config.cache_limit.clone();
        let mut result: Option<&ImageMetadata>;
        let metadata_cache: &mut Vec<ImageMetadata> = &mut data.get_mut::<Metadatacache>().unwrap();
        for hash in hashes {
            result = metadata_cache.iter()
                .find(|i| hash.dist(&i.hash) < 2);
            if result.is_none() {
                let user = msg.author.clone();
                let msg_link = msg.link().clone();
                let timestamp = msg.timestamp;
                metadata_cache.push(ImageMetadata { hash, timestamp, user, msg_link });
                if metadata_cache.len() > cache_size as usize {
                    metadata_cache.pop();
                }
            } else {
                let utc_now: DateTime<Utc> = Utc::now();
                let days_between = utc_now.signed_duration_since(result.unwrap().timestamp).num_days();
                let mut days_between_str = String::from(" posted this ");
                if days_between < 1 {
                    days_between_str.push_str("earlier today: ");
                } else {
                    days_between_str.push_str(&*format!("{} days ago: ", days_between));
                }
                let msg_content = MessageBuilder::new()
                    .push("That's a repost! ")
                    .mention(&result.unwrap().user)
                    .push(days_between_str)
                    .push(&result.unwrap().msg_link)
                    .build();
                if let Err(e) = msg.reply_mention(&context.http, msg_content).await {
                    println!("{}", e);
                }
            }
        }
    }
}