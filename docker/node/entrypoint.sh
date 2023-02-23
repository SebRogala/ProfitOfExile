#!/usr/bin/env bash

function helper {
	echo -e "Unknown environment"
}

function watch {
	cd /var/www
	npm install
	npm run-script watch
}

function build {
	cd /var/www
	npm install
	npm run-script build
}

echo $APP_ENV

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
