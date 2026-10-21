# syntax=docker/dockerfile:1.18@sha256:dabfc0969b935b2080555ace70ee69a5261af8a8f1b4df97b9e7fbcf6722eddf
FROM node:22.15.0-bookworm-slim@sha256:557e52a0fcb928ee113df7e1fb5d4f60c1341dbda53f55e3d815ca10807efdce AS build
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci
COPY index.html login.html routes.html saved.html screens.html tsconfig.json vite.config.js ./
COPY public ./public
COPY scripts ./scripts
COPY src ./src
COPY worker ./worker
COPY .openai ./.openai
RUN npm run build

FROM nginxinc/nginx-unprivileged:1.29.5-alpine@sha256:42a7d7f2ee23e9f5a1dcdf3647ba5c585bbd18f79e79cd817e70e8cd61c55779
COPY --chown=101:101 deploy/edge/nginx.conf /etc/nginx/nginx.conf
COPY --from=build --chown=101:101 /src/dist/client /usr/share/nginx/html
USER 101
EXPOSE 8080
STOPSIGNAL SIGQUIT
