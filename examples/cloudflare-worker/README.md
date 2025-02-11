# cloudflare worker with xitca-web

## Requirement
- nightly Rust
- [cloudflare workers](https://workers.cloudflare.com/)
- [wrangler v1](https://crates.io/crates/wrangler)

## API
Same as auto generated worker template with additional static stie.
```
GET  /
POST /form/<field_name_string> 
GET  /worker-version
```

## Example site
https://xitca-web-worker.fakeshadow.workers.dev/

## Usage
```bash
# compiles your project to WebAssembly and will warn of any issues
wrangler build 

# run your Worker in an ideal development workflow (with a local server, file watcher & more)
wrangler dev

# deploy your Worker globally to the Cloudflare network (update your wrangler.toml file for configuration)
wrangler publish
```
Read the latest `worker` crate documentation here: https://docs.rs/worker
