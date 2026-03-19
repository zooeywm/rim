.PHONY: fmt-toml check-toml

fmt-toml:
	taplo fmt $(shell git ls-files '*.toml')

check-toml:
	taplo fmt --check $(shell git ls-files '*.toml')
