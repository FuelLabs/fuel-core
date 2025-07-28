# The configuration of the V36 network

This configuration was generated in the following way:
1. Get the `fuel-core` from `release/v0.36.0` branch (commit b5d5ff8d13f16a0bd53e79ee0e8f5438971be2b9)
2. Build and run in order to create a DB
3. Run again with the `snapshot` parameter
4. Apply the following modifications to `chain_config.json`
  - PoA Key: `{"address":"3ba3b213c8d5ec4d4901ac34b0e924d635384a8b0a5df6e116bce9a783da8e02","secret":"e3d6eb39607650e22f0befa26d52e921d2e7924d0e165f38ffa8d9d0ac73de93","type":"block_production"}`
  - Privileged Key: `{"address":"f034f7859dbf1d775cba16fc175eef249a045d6484a8b9388c9e280267125b73","secret":"dcbe36d8e890d7489b6e1be442eab98ae2fdbb5c7d77e1f9e1e12a545852304f","type":"block_production"}`
  - Change "chain_name" from "Local testnet" to "V36"