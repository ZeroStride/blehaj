/// <reference types="web-bluetooth" />

import React from 'react';
import logo from './logo.svg';
import './App.css';

function DoBluetoothThings() {
  const queryParams = new URLSearchParams(window.location.search);
  const uuid = queryParams.get("uuid") || "7c59fa4a-3355-4e71-9fe8-e6b2ff4018f9";

  navigator.bluetooth
    .requestDevice({filters:[
      { services: [uuid]}
    ]})
    .then((device) => {
      console.log(`Name: ${device.name}`);
    })
    .catch((error) => console.error(`Something went wrong. ${error}`));
}

function App() {
  return (
    <div className="App">
      <header className="App-header">
        <img src={logo} className="App-logo" alt="logo" />
        <button onClick={DoBluetoothThings}>Do the Bluetooth Things</button>
      </header>
    </div>
  );
}

export default App;
