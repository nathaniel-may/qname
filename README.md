# qname

Organizing your media with keywords and tags across file formats traditionally requires 3rd party system lockin because the app stores metadata in a separate system to organize your files. Working within pre-defined metadata such as EXIF isn't possible across all file formats, and many popular tools strip that data because of its inconsitencies. The one thing every file has is a filename. This app lets you define a keword schema, and it encodes it into the filename so it can be queried by this app or whatever system you choose to store your files in. No lock-in, no incompatibilites.

# usage

_early stages of development- expect nothing to work_

Have a directory with the media files you'd like to organize and include a file named `schema.q` in that directory which describes your desired schema. Run the app like so:

```
cargo run <path>
```

Future Features
- Query the filenames that match the schema.
- Problem: you stop half way through and want to move out the named ones. Solution:??? (ideas: put renamed ones in another folder? or skip ones that match the schema? but what about going backwards to fix one)

# build
For a smaller binary, build with some nightly features. On my laptop it cuts the size down by more than half.
```
cargo +nightly build -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort --target aarch64-apple-darwin --release
```

I tried UPX for even smaller artifacts, but it looks like [MacOS 13+ prevents them from running](https://github.com/upx/upx/issues/612)- an undocumented feature.

# install
On my machine I use
```
sudo cp target/aarch64-apple-darwin/release/qname /usr/local/bin/qname
```
