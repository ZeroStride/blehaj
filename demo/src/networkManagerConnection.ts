import { v5 as uuidv5 } from "uuid"

// This is a random uuidv4, it's nothing special
const NETWORK_MANAGER_ID_NAMESPACE = "4c411299-551a-4fa2-b6e1-b26fef412004"

type DbusVariant = {
    readonly signature: string
    readonly value: string | boolean | Uint8Array
}

type DbusRecord = { [key: string]: DbusVariant | DbusRecord }

interface NetworkManagerConnection {
    readonly settings: DbusRecord
    toString(): string
    toBytes(): Uint8Array<ArrayBuffer>
    toBytesCompressed(): Promise<ArrayBuffer>
}

// https://networkmanager.dev/docs/api/latest/ref-settings.html
export class NetworkManagerConnectionBuilder {
    private settings: DbusRecord

    private constructor() {
        this.settings = {
            "ipv4": {
                "method": {
                    "signature": "s",
                    "value": "auto"
                }
            },
            "ipv6": {
                "method": {
                    "signature": "s",
                    "value": "auto"
                }
            }
        }
    }

    build(): NetworkManagerConnection {
        const settings = this.settings
        const toString = () => JSON.stringify(settings)
        const toBytes = () => {
            const utf8encoder = new TextEncoder()
            return utf8encoder.encode(toString())
        }
        return {
            settings,
            toString,
            toBytes,
            toBytesCompressed: async () => {
                const cs = new CompressionStream("gzip")
                const writer = cs.writable.getWriter()
                writer.write(toBytes())
                writer.close()
                return await new Response(cs.readable).arrayBuffer()
            }
        }
    }

    // https://networkmanager.dev/docs/api/latest/settings-802-11-wireless.html
    static wireless802_11(ssid: string) {
        const builder = new NetworkManagerConnectionBuilder()
        builder.settings["connection"] = {
            "uuid": {
                "signature": "s",
                "value": uuidv5(ssid, NETWORK_MANAGER_ID_NAMESPACE)
            },
            "autoconnect": {
                "signature": "b",
                "value": true
            },
            "id": {
                "signature": "s",
                "value": ssid
            },
            "type": {
                "signature": "s",
                "value": "802-11-wireless"
            }
        }
        builder.settings["802-11-wireless"] = {
            "ssid": {
                "signature": "s",
                "value": ssid
            },
            "mode": {
                "signature": "s",
                "value": "infrastructure"
            }
        }
        return builder
    }

    // https://networkmanager.dev/docs/api/latest/settings-802-11-wireless-security.html
    withWPA_PSK(password: string) {
        if (!this.settings["802-11-wireless"]) {
            throw new TypeError("'withWPA_PSK' only valid for builders created with 'wireless802_11'")
        }
        this.settings["802-11-wireless-security"] = {
            "key-mgmt": {
                "signature": "s",
                "value": "wpa-psk"
            },
            "psk": {
                "signature": "s",
                "value": password
            }
        }
        return this
    }
}
