import { buildApp } from '../src/create-app.js';

let app;

export default async function handler(req, res) {
  if (!app) {
    app = await buildApp();
    await app.ready();
  }
  await app.routing(req, res);
}
