#!/usr/bin/env bash

function helper {
	echo -e "Unknown environment"
}

function watch {
	cd /var/www
	yarn
	yarn watch
}

function build {
	cd /var/www
	yarn
	yarn build
}

case $APP_ENV in
	'prod')
		build
		;;
	'dev')
		watch
		;;
	*)
		helper
		;;
esac
