use std::collections::HashSet;
use std::env;

use chrono::{DateTime, Utc};
use image;
use img_hash::{HasherConfig, ImageHash};
use regex::Regex;
use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::StandardFramework;
use serenity::model::channel::{Attachment, Message};
use serenity::model::user::User;
use serenity::prelude::TypeMapKey;
use serenity::utils::MessageBuilder;

struct Handler;

#[derive(PartialEq, Eq, Hash)]
struct ImageMetadata {
    hash: ImageHash,
    timestamp: DateTime<Utc>,
    user: User,
    msg_link: String,
}

#[derive(PartialEq, Eq, Hash)]
struct LinkMetadata {
    url: String,
    timestamp: DateTime<Utc>,
    user: User,
    msg_link: String,
}

struct HashCache;

struct LinkCache;

struct AllowedLinks;

struct AllowedHashes;

struct Config {
    cache_limit: u64,
}

impl TypeMapKey for HashCache {
    type Value = HashSet<ImageMetadata>;
}

impl TypeMapKey for LinkCache {
    type Value = HashSet<LinkMetadata>;
}

impl TypeMapKey for Config {
    type Value = Config;
}

impl TypeMapKey for AllowedLinks {
    type Value = HashSet<String>;
}

impl TypeMapKey for AllowedHashes {
    type Value = HashSet<ImageHash>;
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
        let config = read_config();
        data.insert::<Config>(config);
        data.insert::<HashCache>(HashSet::with_capacity(read_config().cache_limit as usize));
        data.insert::<LinkCache>(HashSet::with_capacity(read_config().cache_limit as usize));
        data.insert::<AllowedHashes>(HashSet::with_capacity(read_config().cache_limit as usize));
        data.insert::<AllowedLinks>(HashSet::with_capacity(read_config().cache_limit as usize));
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
        if msg.author.bot { return; }
        let url_regex = Regex::new("https?://(www\\.)?[-a-zA-Z0-9@:%._+~#=]{2,256}\\.[a-z]{2,4}\\b([-a-zA-Z0-9@:%_+.~#?&/=]*)").unwrap();
        if msg.content.contains("--allow") {
            set_allowed(&context, &msg, &url_regex).await;
            return;
        }
        let mut data = context.data.write().await;
        let allowed_links: HashSet<String> = data.get::<AllowedLinks>().unwrap().clone();
        let attachments: Vec<&Attachment> = msg.attachments.iter()
            .map(|a| a)
            .collect();
        let embedded: Vec<String> = msg.embeds.iter()
            .map(|e| e.url.clone())
            .filter(|e| e.is_some())
            .map(|e| e.unwrap())
            .filter(|str| !allowed_links.contains(str))
            .collect();
        if attachments.is_empty() && embedded.is_empty() && !url_regex.is_match(&msg.content) {
            return;
        }
        let link_cache: &mut HashSet<LinkMetadata> = &mut data.get_mut::<LinkCache>().unwrap();
        let url_matches: HashSet<&str> = url_regex.find_iter(&msg.content)
            .map(|mat| mat.as_str())
            .filter(|str| !allowed_links.contains(&*str.to_string()))
            .collect();
        for url in url_matches {
            let link = link_cache.iter().find(|l| l.url == url);
            if link.is_some() {
                let utc_now: DateTime<Utc> = Utc::now();
                let days_between = utc_now.signed_duration_since(link.unwrap().timestamp).num_days();
                let mut days_between_str = String::from(" posted this ");
                if days_between < 1 {
                    days_between_str.push_str("earlier today: ");
                } else {
                    days_between_str.push_str(&*format!("{} days ago: ", days_between));
                }
                let msg_content = MessageBuilder::new()
                    .push("That's a repost! ")
                    .mention(&link.unwrap().user)
                    .push(days_between_str)
                    .push(&link.unwrap().msg_link)
                    .build();
                if let Err(e) = msg.reply_mention(&context.http, msg_content).await {
                    println!("{}", e);
                }
            } else {
                let url = url.to_string();
                let user = msg.author.clone();
                let msg_link = msg.link().clone();
                let timestamp = msg.timestamp;
                link_cache.insert(LinkMetadata { url, timestamp, user, msg_link });
            }
        }
        let mut hashes: Vec<ImageHash> = Vec::new();
        let allowed_hashes: HashSet<ImageHash> = data.get::<AllowedHashes>().unwrap().clone();
        for url in embedded {
            if let Some(image_hash) = get_embedded_hash(url).await {
                if !allowed_hashes.contains(&image_hash) {
                    hashes.push(image_hash);
                }
            }
        }
        for attachment in attachments {
            if let Some(image_hash) = get_attachment_hash(attachment.clone()).await {
                if !allowed_hashes.contains(&image_hash) {
                    hashes.push(image_hash);
                }
            }
        }
        let mut result: Option<&ImageMetadata>;
        let metadata_cache: &mut HashSet<ImageMetadata> = &mut data.get_mut::<HashCache>().unwrap();
        for hash in hashes {
            result = metadata_cache.iter()
                .find(|i| hash.dist(&i.hash) < 2);
            if result.is_none() {
                let user = msg.author.clone();
                let msg_link = msg.link().clone();
                let timestamp = msg.timestamp;
                metadata_cache.insert(ImageMetadata { hash, timestamp, user, msg_link });
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

async fn get_embedded_hash(url: String) -> Option<ImageHash> {
    if let Ok(resp) = reqwest::get(&url).await {
        if let Ok(img_bytes) = resp.bytes().await {
            if let Ok(img) = &image::load_from_memory(img_bytes.as_ref()) {
                let image_hash = HasherConfig::new()
                    .to_hasher()
                    .hash_image(img);
                return Some(image_hash);
            }
        }
    }
    None
}

async fn get_attachment_hash(attachment: Attachment) -> Option<ImageHash> {
    if let Ok(img) = attachment.download().await {
        if let Ok(img) = &image::load_from_memory(img.as_ref()) {
            return Some(HasherConfig::new()
                .to_hasher()
                .hash_image(img));
        }
    }
    None
}

async fn set_allowed(context: &Context, msg: &Message, url_regex: &Regex) {
    let mut data = context.data.write().await;
    if url_regex.is_match(&*msg.content) {
        let allowed_links: &mut HashSet<String> = data.get_mut::<AllowedLinks>().unwrap();
        let url_matches: HashSet<&str> = url_regex.find_iter(&msg.content)
            .map(|mat| mat.as_str())
            .collect();
        for url in url_matches {
            if let Ok(url) = url.parse() {
                allowed_links.insert(url);
            }
        }
    }
    let allowed_hashes: &mut HashSet<ImageHash> = data.get_mut::<AllowedHashes>().unwrap();
    for attachment in &msg.attachments {
        if let Some(img_hash) = get_attachment_hash(attachment.clone()).await {
            allowed_hashes.insert(img_hash);
        }
    }
    let allowed_links: &mut HashSet<String> = data.get_mut::<AllowedLinks>().unwrap();
    for embed in &msg.embeds {
        if let Some(url) = embed.clone().url {
            allowed_links.insert(url);
        }
    }
}