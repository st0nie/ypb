#!/bin/bash

apk add --update nodejs npm
npm install tailwindcss @tailwindcss/cli
npx @tailwindcss/cli -i ./web/style.css -o ./web/output.css --minify