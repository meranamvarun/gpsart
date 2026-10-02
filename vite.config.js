import { defineConfig } from 'vite';
import { resolve } from 'node:path';
export default defineConfig({build:{outDir:'dist/client',rollupOptions:{input:{index:resolve('index.html'),login:resolve('login.html'),routes:resolve('routes.html'),saved:resolve('saved.html'),screens:resolve('screens.html')}}},server:{host:'0.0.0.0',allowedHosts:['terminal.local']}});
