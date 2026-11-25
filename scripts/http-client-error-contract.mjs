import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http";
import test from "node:test";

export function runHttpClientErrorContract(label, createClient) {
  test(`${label}: object and link errors follow HTTP status`, async (t) => {
    let reply;
    const server = createServer((req, res) => {
      if (req.url.endsWith("/transport")) {
        req.socket.destroy();
        return;
      }
      res.writeHead(reply.status, { "content-type": reply.contentType ?? "application/json" });
      res.end(typeof reply.body === "string" ? reply.body : JSON.stringify(reply.body));
    });
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    t.after(() => new Promise((resolve, reject) => {
      server.close((error) => error ? reject(error) : resolve());
      server.closeAllConnections();
    }));
    const client = createClient(`http://127.0.0.1:${server.address().port}`);
    const lookups = [
      ["object", (id = "missing") => client.get("notes", id)],
      ["link", (id = "missing") => client.getLink(id)],
    ];

    for (const [kind, lookup] of lookups) {
      await t.test(`${kind}: actual 404s return null for every error envelope`, async () => {
        for (const body of [
          { error: { message: "Record not found" } },
          { error: { detail: "Record not found" } },
          { error: "Record not found" },
          "<html>Not found</html>",
        ]) {
          reply = { status: 404, body };
          assert.equal(await lookup(), null);
        }
      });

      await t.test(`${kind}: other HTTP errors retain status and useful messages`, async () => {
        for (const status of [400, 401, 500]) {
          for (const error of [
            { message: "Failed to read record 404" },
            { detail: "Failed to read record 404" },
            "Failed to read record 404",
          ]) {
            reply = { status, body: { error } };
            await assert.rejects(lookup(), (failure) => {
              assert.ok(failure instanceof Error);
              assert.equal(failure.status, status);
              assert.equal(failure.message, "Failed to read record 404");
              return true;
            });
          }
        }
        reply = { status: 503, body: "<html>Unavailable</html>", contentType: "text/html" };
        await assert.rejects(lookup(), (failure) => failure.status === 503 && failure.message === "HTTP 503");
      });

      await t.test(`${kind}: transport failures still throw`, async () => {
        await assert.rejects(lookup("transport"), TypeError);
      });
    }
  });
}
