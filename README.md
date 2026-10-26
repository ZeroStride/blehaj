# blehaj 🦈

## Signing the Configuration Request

When making a configuration request, it can optionally include a digest that is signed with a provided private key. This allows the server to verify the identity of the configuration request.

To enable this functionality, ensure the binary is built with the `digest` feature (enabled by default), and provide the path to the private key via the `--signing-key` parameter. It is *strongly recommended* that this file be provided using `LoadCredentialEncrypted` or `SetCredentialEncrypted` in the service description.

More information can be found here: https://systemd.io/CREDENTIALS/

## Todo

- [x] Come up with a name
- [ ] Rename the damn keys from `unnamed_ed25519` to something else
- [ ] Move to Codeberg?
- [ ] Add actual encryption to the configuration request, instead of just digest signing (optional?)

# Other Interesting Things

- https://github.com/espressif/esp-idf-provisioning-ios
