The core RESTful functionality is powered by [axum](https://github.com/tokio-rs/axum)'s [Router](https://docs.rs/axum/latest/axum/struct.Router.html) - simple and extremely flexible framework to compose routes and middleware. Prest adds a couple of utils to it to simplify common host needs: server startup with a bunch of common middleware, embedding files by lazily reading from disk in debug mode and including into the binary for the releases based on [rust-embed](https://github.com/pyrossh/rust-embed), easy global state variables with [Lazy](https://docs.rs/once_cell/latest/once_cell/sync/struct.Lazy.html) initialization (even async ones with [block_on](https://docs.rs/futures-executor/latest/futures_executor/fn.block_on.html)), [anyhow](https://github.com/dtolnay/anyhow)'s [Result](https://docs.rs/anyhow/latest/anyhow/type.Result.html) for simple error handling, and a couple of others. Everything is either exposed, re-exported or used under the hood so that you only need to add a single dependency:

`/Cargo.toml`
{Cargo.toml}

While axum has built-in helpers for the state management, they can introduce type-related issues when you're merging and nesting routers as we will do later with shared router for both host and client. So, I recommend using good old rust statics for state variables like DB connections and others, which also have a nice property of having the initializaiton logic right in the declaration. This example showcases the basic structure of the prest host:

`/serve.rs`
{serve.rs}

Once started it will compose the router, attempt to get the `PORT` env variable or default to `80`, set up common middleware for tracing, compression and limiting request bodies, connect to the socket and start processing requests. You can check out the root path (`/`) which returns extracted host and state info, as well as the header added by the middleware. Also, you can check out the `/Cargo.toml` and `/serve.rs` paths of the running app to see the contents of the embedded files.

In some cases you might want to have lower-level control - for example to configure proxy or customize the runtime settings. In these cases you can easily import underlying crates directly and use only those prest utils which fit your needs. Under the hood its powered by [tokio](https://docs.rs/tokio/latest/tokio/) - general-purpose async runtime which provides exceptional performance for a wide range of applications, [hyper](https://hyper.rs/) for extremely reliable and efficient HTTP processing and [tower-http](https://github.com/tower-rs/tower-http) for generic middleware. Prest, as well as most of the rust web ecosystem, also relies on [http](https://docs.rs/http/latest/http/) and [tower](https://docs.rs/tower/latest/tower/) crates for common types and compatability.

Starting with this code you can already include a bunch of html, css and js assets and get your website started. However, it won't be particularly convenient for development or the users, so let's move on to the next example where we'll explore a way to work with hypermedia without leaving the comfort of rust while also improving UX.

[Next: hello-html](https://prest.blog/hello-html)