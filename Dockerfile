FROM node:24-alpine AS build
RUN corepack enable
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY tsconfig.json ./
COPY src ./src
RUN pnpm run build && pnpm prune --prod

FROM node:24-alpine
ENV NODE_ENV=production
WORKDIR /app
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist
COPY package.json ./
COPY catalog ./catalog
USER node
EXPOSE 8484
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- http://127.0.0.1:8484/healthz || exit 1
ENTRYPOINT ["node", "dist/main.js"]
CMD ["serve"]
