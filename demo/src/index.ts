/// <reference types="node" />

import { decode, decodeAsync } from "@msgpack/msgpack"
import { serve } from "bun"
import index from "./index.html"

interface BleRequest {
  identifier: string,
  data: Uint8Array,
}

interface BleEnvelope {
  signature: string,
  payload: Uint8Array,
}

const server = serve({
  routes: {
    // Serve index.html for all unmatched routes.
    "/*": index,

    "/api/hello": {
      async GET(req) {
        return Response.json({
          message: "Hello, world!",
          method: "GET",
        })
      },
      async PUT(req) {
        return Response.json({
          message: "Hello, world!",
          method: "PUT",
        })
      },
    },

    "/api/hello/:name": async req => {
      const name = req.params.name
      return Response.json({
        message: `Hello, ${name}!`,
      })
    },

    "/api/configuration": {
      async POST(req) {
        if (!req.body) {
          return Response.json({
            error: "empty body"
          }, {
            status: 404
          })
        }

        await decodeAsync(req.body)
          .then((decoded_raw) => decoded_raw as [string, Uint8Array])
          .then((decoded) => {
            console.log(decoded)
            return {
              signature: decoded[0],
              payload: decoded[1]
            } as BleEnvelope
          })
          .then((envelope) => {
            const request_decoded = decode(envelope.payload) as [string, Uint8Array]
            const request = {
              identifier: request_decoded[0],
              data: request_decoded[1],
            } as BleRequest

            if (envelope.signature) {
              console.log(`signature included with request: ${envelope.signature}`)
              const sig_bytes = Uint8Array.fromHex(envelope.signature)

              // Todo - This would fetch from the database using 'identifier'
              const todoFetchFromDatabaseOrWhatever = Promise.resolve(Uint8Array.fromHex("731e93151e81e645fd6ba7642c57d7551d9fb5d167f796e039727c97e1f1a70e"))

              return todoFetchFromDatabaseOrWhatever
                .then((key_bytes) => crypto.subtle.importKey("raw", key_bytes, { "name": "Ed25519" }, false, ["verify"]))
                .then((key) => crypto.subtle.verify({ "name": "Ed25519" }, key, sig_bytes, Buffer.from(envelope.payload)))
                .then((matched) => {
                  if (!matched) {
                    throw new Error("signature mismatch")
                  }

                  return request
                })
            } else {
              return request
            }
          })
          .then(configurationRequest => console.log(configurationRequest))

        return Response.json({
          message: "ok"
        })
      }
    }
  },

  development: process.env.NODE_ENV !== "production" && {
    // Enable browser hot reloading in development
    hmr: true,

    // Echo console logs from the browser to the server
    console: true,
  },
})

console.log(`🚀 Server running at ${server.url}`)
