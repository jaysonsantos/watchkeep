FROM node:24-alpine AS build
RUN corepack enable
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY tsconfig.json tsconfig.server.json svelte.config.js vite.config.ts ./
COPY src ./src
COPY static ./static
RUN pnpm exec svelte-kit sync && pnpm run build && pnpm prune --prod

FROM node:24-alpine
ENV NODE_ENV=production
WORKDIR /app
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist
COPY --from=build /app/build ./build
COPY package.json ./
COPY catalog ./catalog
USER node
EXPOSE 8484
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- http://127.0.0.1:8484/healthz || exit 1
ENTRYPOINT ["node", "dist/server/main.js"]
CMD ["serve"]
