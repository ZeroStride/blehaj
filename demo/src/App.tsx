/// <reference types="web-bluetooth" />
import { Scanner, type IDetectedBarcode } from "@yudiel/react-qr-scanner"
import { useState } from "react"
import { APITester } from "./APITester"
import "./index.css"
import logo from "./logo.svg"
import { NetworkManagerConnectionBuilder } from "./networkManagerConnection"
import reactLogo from "./react.svg"

export function App() {
  const CONFIGURATION_CHARACTERISTIC_UUID = "4b3bd2c3-2560-4353-8d37-05ca8333d01e"
  const queryParams = new URLSearchParams(window.location.search)
  const uuid = queryParams.get("uuid")

  // Todo - Should probably do an actual state machine here or something
  const [configurationCharacteristic, setConfigurationCharacteristic] = useState<BluetoothRemoteGATTCharacteristic>()

  return (
    <div className="app">
      <div className="logo-container">
        <img src={logo} alt="Bun Logo" className="logo bun-logo" />
        <img src={reactLogo} alt="React Logo" className="logo react-logo" />
      </div>

      {!uuid && (<div>No UUID in URL Param; can't do the thing</div>)}
      {!configurationCharacteristic && uuid && (<button onClick={() => {
        navigator.bluetooth
          .requestDevice({
            filters: [
              { services: [uuid] }
            ]
          })
          .then(device => {
            console.log(`Got device ${device} connecting to ${device.gatt}...`)
            return device.gatt?.connect()
          })
          .then(server => {
            console.log(`Connected, getting service ${uuid}`)
            return server?.getPrimaryService(uuid)
          })
          .then(service => {
            console.log(`Service found ${service}, connecting to characteristic ${CONFIGURATION_CHARACTERISTIC_UUID}`)
            return service?.getCharacteristic(CONFIGURATION_CHARACTERISTIC_UUID)
          })
          .then(characteristic => {
            console.log(`Characteristic found ${characteristic}, reading configuration requirements.`)
            setConfigurationCharacteristic(characteristic)
            return characteristic?.readValue()
          })
          .then(value => {
            const decoder = new TextDecoder("utf-8")
            console.log(`Requirements: ${decoder.decode(value)}`)
          })
          .catch((error) => console.error(`Something went wrong. ${error}`))
      }}>Do the Bluetooth Things</button>)}
      {configurationCharacteristic && (<Scanner
        onScan={async (detectedCodes: IDetectedBarcode[]) => {
          const qr_match = /WIFI:S:(.+);T:(\w+);P:(.*);H:/
          for (var i = detectedCodes.length - 1; i >= 0; i--) {
            const match = detectedCodes[i]?.rawValue.match(qr_match)
            if (match && match[1] && match[2]) {
              let builder = NetworkManagerConnectionBuilder.wireless802_11(match[1])
              if (match[2].toLowerCase() === "wpa") {
                if (!match[3]) {
                  throw new TypeError("password must be specified when security type is 'wpa'")
                }
                builder = builder.withWPA_PSK(match[3])
              } else {
                throw new Error(`todo: implement '${match[2]}' security type`)
              }

              const configuration = builder.build()
              console.log(configuration.settings)
              console.log(configuration.toString())
              const bytes = await configuration.toBytesCompressed()
              console.log(`Writing configuration as json (${bytes.byteLength} bytes, compressed)`)
              configurationCharacteristic.writeValue(bytes)
              break
            }
          }
        }}
        onError={(error) => console.log(error?.message)}
        sound="false"
      />)}
      <APITester />
    </div>
  )
}

export default App
