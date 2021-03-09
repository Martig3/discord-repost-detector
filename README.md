# discord-repost-detector
Rust based Discord bot that detects reposted images & links.

## Config
Set the following environment variables:
```
REPOST_DISCORD_TOKEN=your_discord_bot_token
REPOST_CACHE_LIMIT=5000
REPOST_IGNORED_TYPES=link,attachment    --these are the two options, use only one 
```
`REPOST_CACHE_LIMIT` can be set to any 64bit unsigned integer.

## Run

Standard `cargo run`

## Commands
Use the `--allow` command with any link or image to exclude it from detection.


