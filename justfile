bundle:
	cargo bundle --release
	cp -R ./target/release/bundle/osx/Newt3.app /Applications
