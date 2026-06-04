#!/usr/bin/env bun
/** Bridge script: stdin JSON -> headroom-ai compress() -> stdout JSON. */

import { compress } from "headroom-ai";

function messageContent(message) {
  if (!message) return "";
  const content = message.content;
  if (content == null) return "";
  return typeof content === "string" ? content : String(content);
}

function readStdinJson() {
  return new Promise((resolve, reject) => {
    const chunks = [];
    process.stdin.on("data", (chunk) => chunks.push(chunk));
    process.stdin.on("end", () => {
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (error) {
        reject(error);
      }
    });
    process.stdin.on("error", reject);
  });
}

function compressStats(result) {
  return {
    tokensBefore: result.tokensBefore ?? null,
    tokensAfter: result.tokensAfter ?? null,
    tokensSaved: result.tokensSaved ?? null,
    compressionRatio: result.compressionRatio ?? null,
    transformsApplied: result.transformsApplied ?? [],
    ccrHashes: result.ccrHashes ?? [],
  };
}

async function compressSingleToolOutput(payload) {
  const content = payload?.content;
  if (typeof content !== "string") {
    console.log(JSON.stringify({ error: "missing string field: content" }));
    process.exit(1);
  }

  const model = payload.model || "gpt-4o";
  const messages = [
    {
      role: "tool",
      tool_call_id: payload.toolCallId || "bash-output",
      content,
    },
  ];

  const result = await compress(messages, { model, fallback: false });
  const compressed = result.messages?.length
    ? messageContent(result.messages[0]) || content
    : content;

  console.log(
    JSON.stringify({
      content: compressed,
      ...compressStats(result),
    }),
  );
}

async function compressFullMessages(payload) {
  const messages = payload?.messages;
  if (!Array.isArray(messages) || messages.length === 0) {
    console.log(JSON.stringify({ error: "missing non-empty messages array" }));
    process.exit(1);
  }

  const model = payload.model || "gpt-4o";
  const result = await compress(messages, { model, fallback: false });
  const compressedMessages = Array.isArray(result.messages) ? result.messages : messages;

  console.log(
    JSON.stringify({
      messages: compressedMessages,
      ...compressStats(result),
    }),
  );
}

async function main() {
  let payload;
  try {
    payload = await readStdinJson();
  } catch (error) {
    console.log(JSON.stringify({ error: `invalid stdin json: ${error}` }));
    process.exit(1);
  }

  try {
    if (payload?.mode === "messages") {
      await compressFullMessages(payload);
    } else {
      await compressSingleToolOutput(payload);
    }
    process.exit(0);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.log(JSON.stringify({ error: `headroom.compress failed: ${message}` }));
    process.exit(1);
  }
}

await main();
