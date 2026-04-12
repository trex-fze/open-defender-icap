FROM node:20-alpine AS build

WORKDIR /app/web-admin

COPY web-admin/package.json web-admin/package-lock.json ./
RUN npm ci

COPY web-admin/ ./
RUN npm run build

FROM nginx:1.25-alpine

COPY deploy/docker/web-admin/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/web-admin/dist /usr/share/nginx/html

EXPOSE 19001

CMD ["nginx", "-g", "daemon off;"]
