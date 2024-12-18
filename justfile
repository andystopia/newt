bundle:
	cd crates/newt-gui && cargo bundle --release --bin newt-gui
	cp -R ./target/release/bundle/osx/Newt3.app /Applications
