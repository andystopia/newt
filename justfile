bundle:
	cd crates/newt-gui && cargo bundle --release --bin newt-gui
	cp -R ./target/release/bundle/osx/Newt3.app /Applications
install-gnix:
	cargo install --path crates/gnix
