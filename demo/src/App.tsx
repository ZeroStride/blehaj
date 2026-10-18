/// <reference types="web-bluetooth" />

import React from 'react';
import { useState } from 'react';
import logo from './logo.svg';
import './App.css';
import { type IDetectedBarcode, Scanner } from '@yudiel/react-qr-scanner';

function ConfigurationJsonForAP(ssid:String, security:String, password:String | null) {
  const network_manager_json = {
    "connection": {
      "uuid": {
        "signature": "s",
        "value": "8b0d053a-cb44-4953-85df-6ef3229c9581"
      },
      "autoconnect": {
        "signature": "b",
        "value": true
      },
      "id": {
        "signature": "s",
        "value": "ssid"
      },
      "type": {
        "signature": "s",
        "value": "802-11-wireless"
      }
    },
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
    },
    "802-11-wireless": {
      "ssid": {
        "signature": "s",
        "value": "ssid"
      },
      "mode": {
        "signature": "s",
        "value": "infrastructure"
      }
    },
    "802-11-wireless-security": {
      "key-mgmt": {
        "signature": "s",
        "value": "wpa-psk"
      },
      "psk": {
        "signature": "s",
        "value": "password"
      }
    }
  };

  return {
    ...network_manager_json,
    ...{
      "connection": {
        "uuid": {
          "signature": "s",
          "value": "8b0d053a-cb44-4953-85df-6ef3229c9581" // Todo
        },
        "id": {
          "signature": "s",
          "value": ssid
        },
      },
      "802-11-wireless": {
        "ssid": {
          "signature": "s",
          "value": ssid
        },
      },
      "802-11-wireless-security": {
        "key-mgmt": {
          "signature": "s",
          "value": "wpa-psk" // Todo
        },
        "psk": {
          "signature": "s",
          "value": password
        }
      }
    }
  };
}

function App() {
  const CONFIGURATION_CHARACTERISTIC_UUID = "4b3bd2c3-2560-4353-8d37-05ca8333d01e";
  const queryParams = new URLSearchParams(window.location.search);
  const uuid = queryParams.get("uuid");

  // Todo - Should probably do an actual state machine here or something
  const [configurationCharacteristic, setConfigurationCharacteristic] = useState<BluetoothRemoteGATTCharacteristic>();

  return (
    <div className="App">
      <header className="App-header">
        <img src={logo} className="App-logo" alt="logo" />
        {!uuid && (<div>No UUID in URL Param; can't do the thing</div>)}
        {!configurationCharacteristic && uuid && (<button onClick={() => {
          navigator.bluetooth
            .requestDevice({filters:[
              { services: [uuid] }
            ]})
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
              setConfigurationCharacteristic(characteristic);
              return characteristic?.readValue()
            })
            .then(value => {
              const decoder = new TextDecoder('utf-8');
              console.log(`Requirements: ${decoder.decode(value)}`);
            })
            .catch((error) => console.error(`Something went wrong. ${error}`));
        }}>Do the Bluetooth Things</button>)}
        {configurationCharacteristic && (<Scanner
          onScan={(detectedCodes:IDetectedBarcode[]) => {
            const qr_match = /WIFI:S:(.+);T:(\w+);P:(.*);H:/;
            for (var i = detectedCodes.length - 1; i >= 0; i--) {
              const match = detectedCodes[i].rawValue.match(qr_match);
              if (match) {
                const configurationJson = ConfigurationJsonForAP(match[1], match[2], match[3]);
                console.log(configurationJson);
                const utf8encoder = new TextEncoder();
                configurationCharacteristic.writeValue(utf8encoder.encode(JSON.stringify(configurationJson)));
                break;
              }
            }
          }}
          onError={(error) => console.log(error?.message)}
          sound="false"
        />)}
      </header>
    </div>
  );
}

export default App;
