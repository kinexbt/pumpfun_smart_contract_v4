# JSON RPC HTTP Methods

The block engine accepts HTTP requests using the [JSON-RPC 2.0](https://www.jsonrpc.org/specification) specification.

## RPC HTTP Endpoint

**Default port:** 443 e.g. [https://mainnet.block-engine.jito.wtf:443/api/v1/bundles](https://mainnet.block-engine.jito.wtf:443/api/v1/bundles), [https://{REGION}.mainnet.block-engine.jito.wtf:443/api/v1/bundles](https://${REGION}.mainnet.block-engine.jito.wtf:443/api/v1/bundles)

## Request Formatting

To make a JSON-RPC request, send an HTTP POST request with a `Content-Type: application/json` header. The JSON request data should contain 4 fields:

- `jsonrpc`: `string` - set to "2.0"
- `id`: `number` - a unique client-generated identifying integer
- `method`: `string` - a string containing the method to be invoked
- `params`: `array` - a JSON array of ordered parameter values

We follow the same request formatting as Solana json rpc requests. You can find the documentation [here](https://docs.solana.com/api/http#request-formatting)

## Authorization

In the short-term we don't require authentication to send the requests.

If there are any changes to the authentication mechanism it would be updated in the document and communicated with all the stakeholders.

## Definitions

- Bundle: Bundles are a list of transactions that execute sequentially and atomically. “All or nothing” so to speak. This means that a user can send a bundle that contains multiple transactions and guarantee that they are all executed one after the other and the bundle succeeds only if all individual transactions succeed.
- Hash: A SHA-256 hash of a chunk of data.
- Pubkey: The public key of a Ed25519 key-pair.
- Tip Account: List of accounts to which a tip can be paid for processing the bundles. Clients submitting bundles must pay a tip for bundle processing.
- Transaction: A list of Solana instructions signed by a client keypair to authorize those actions.
- Signature: An Ed25519 signature of transaction's payload data including instructions. This can be used to identify transactions.

## State Commitment

The commitment describes how finalized a block is at that point in time.

In descending order of commitment (most finalized to least finalized), these are the commitment levels:

- `"finalized"` - the node will query the most recent block confirmed by supermajority
  of the cluster as having reached maximum lockout, meaning the cluster has
  recognized this block as finalized
- `"confirmed"` - the node will query the most recent block that has been voted on by supermajority of the cluster.
  - It incorporates votes from gossip and replay.
  - It does not count votes on descendants of a block, only direct votes on that block.
  - This confirmation level also upholds "optimistic confirmation" guarantees in
    release 1.3 and onwards.
- `"processed"` - the node will query its most recent block. Note that the block
  may still be skipped by the cluster.

Please refer to [configuring-state-commitment](https://docs.solana.com/api/http#configuring-state-commitment) for more details.

#### RpcResponse Structure

Many methods that take a commitment parameter return an RpcResponse JSON object comprised of two parts:

- `context` : An RpcResponseContext JSON structure including a `slot` field at which the operation was evaluated. example:

  ```json
      "context": {
      "slot": 1
    },
  ```

- `value` : The value returned by the operation itself. example:

```json
        "value": [
            {
                "bundle": {
                    "signatures": [
                        "3Eq21vXNB5s86c62bVuUfTeaMif1N2kUqRPBmGRJhyTA",
                        "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv"
                    ]
                },
                "slot": 1234,
                "confirmationStatus": "finalized",
                "err": "null"
            }
        ]
```

## JSON RPC API Reference

## getTipAccounts

Returns the tip accounts for tip payment for the bundles.

### Parameters

None

### Result

The result field will be a JSON object with the following fields:

- `result`: `<array>` - Tip accounts as a list of `strings`

### Code sample

#### Request

```bash
curl https://mainnet.block-engine.jito.wtf:443/api/v1/bundles -X POST -H "Content-Type: application/json" -d '
{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getTipAccounts",
    "params": []
}
'
```

#### Response

```json
{
    "jsonrpc": "2.0",
    "result": [
        "9n3d1K5YD2vECAbRFhFFGYNNjiXtHXJWn9F31t89vsAV",
        "aTtUk2DHgLhKZRDjePq6eiHRKC1XXFMBiSUfQ2JNDbN",
        "B1mrQSpdeMU9gCvkJ6VsXVVoYjRGkNA7TtjMyqxrhecH",
        "9ttgPBBhRYFuQccdR1DSnb7hydsWANoDsV3P9kaGMCEh",
        "4xgEmT58RwTNsF5xm2RMYCnR1EVukdK8a1i2qFjnJFu3",
        "EoW3SUQap7ZeynXQ2QJ847aerhxbPVr843uMeTfc9dxM",
        "E2eSqe33tuhAHKTrwky5uEjaVqnb2T9ns6nHHUrN8588",
        "ARTtviJkLLt6cHGQDydfo1Wyk6M4VGZdKZ2ZhdnJL336"
    ],
    "id": 1
}
```

## sendBundle

Submits a bundled list of signed transaction(s) (base-58 encoded string) to the cluster for processing. The transactions will be atomically processed in order, meaning if any of the transactions fail, the entire bundle won’t be processed (all or nothing). This method does not alter the transaction in any way; it relays the bundle created by clients to the leader as-is. If the bundle is not set to expire before the next upcoming Jito-Solana leader, this method will immediately return a success response acknowledging that the bundle has been received with a bundle_id. This does not guarantee the bundle is processed or landed on-chain. For the bundle status regarding whether it landed or not, getBundleStatuses should be used with the bundle id

Please note that a tip is necessary for the bundle to considered. A tip can be any instruction, top-level or CPI, that transfers SOL to one of the 8 tip accounts. Clients should make sure they have balance and state assertions that allow the tip to only go thru conditionally, especially if tipping as a separate tx. If the tip is low, there is a chance that the bundle does not get selected during the auction. You can get the tip accounts using [getTipAccounts](#gettipaccounts). Ideally select one of the accounts in random to reduce contention.

### Parameters

`<array[string]>`: `required` Fully-signed Transactions, as encoded string (base-58) upto a maximum of 5. Please note that at this point, we don't support base-64 encoded transactions

### Result

The result field will be a JSON object with the following fields:

- `result`: `<string>` - A bundle id, used to identify the bundle. This is the Sha256 hashes of the bundle's tx signatures.

### Code sample

#### Request

```bash
curl https://mainnet.block-engine.jito.wtf:443/api/v1/bundles -X POST -H "Content-Type: application/json" -d '
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sendBundle",
  "params": [
    [
      "4VbvoRYXFaXzDBUYfMXP1irhMZ9XRE6F1keS8GbYzKxgdpEasZtRv6GXxbygPp3yBVeSR4wN9JEauSTnVTKjuq3ktM3JpMebYpdGxZWUttJv9N2DzxBm4vhySdq2hbu1LQX7WxS2xsHG6vNwVCjP33Z2ZLP7S5dZujcan1Xq5Z2HibbbK3M3LD59QVuczyK44Fe3k27kVQ43oRH5L7KgpUS1vBoqTd9ZTzC32H62WPHJeLrQiNkmSB668FivXBAfMg13Svgiu9E",
      "6HZu11s3SDBz5ytDj1tyBuoeUnwa1wPoKvq6ffivmfhTGahe3xvGpizJkofHCeDn1UgPN8sLABueKE326aGLXkn5yQyrrpuRF9q1TPZqqBMzcDvoJS1khPBprxnXcxNhMUbV78cS2R8LrCU29wjYk5b4JpVtF23ys4ZBZoNZKmPekAW9odcPVXb9HoMnWvx8xwqd7GsVB56R343vAX6HGUMoiB1WgR9jznG655WiXQTff5gPsCP3QJFTXC7iYEYtrcA3dUeZ3q4YK9ipdYZsgAS9H46i9dhDP2Zx3"
    ]
  ]
}
'
```

#### Response

```json
{
    "jsonrpc": "2.0",
    "result": "2id3YC2jK9G5Wo2phDx4gJVAew8DcY5NAojnVuao8rkxwPYPe8cSwE5GzhEgJA2y8fVjDEo6iR6ykBvDxrTQrtpb",
    "id": 1
}
```

## getBundleStatuses

Returns bundle statuses for submitted bundle(s). The behavior is similar to the solana rpc method [getSignatureStatuses](https://docs.solana.com/api/http#getsignaturestatuses). If the bundle_id is not found or if the bundle has not landed, we return null. If found and landed, we return the context information including the slot at which the request was made and result with the bundle_id(s) and the transactions with the slot and confirmation status.

### Parameters

- `<array[string]>`: `required` An array of bundle ids to confirm, as base-58 encoded strings (up to a maximum of 5).

### Result

An array of RpcResponse`<object>` consisting of either:

- `<null>`: If the bundle is not found.
- `<object>`: If the bundle is found, an array of objects with the following fields:
  - `bundle_id`: `<String>` Bundle id
  - `transactions`: `<array[string]>` - A list of base-58 encoded signatures applied by the bundle. The list will not be empty.
  - `slot`: `<u64>`  The slot this bundle was processed in.
  - `confirmationStatus`: `<string|null>` - The bundle transaction's cluster confirmation status; Either processed, confirmed, or finalized. See [Commitment](#state-commitment) for more on optimistic confirmation.
  - `err`: `<object|null>`: This will show any retryable or non-retryable error encountered when getting the bundle status. If retryable, please query again

### Code sample

### Request

```bash
curl https://mainnet.block-engine.jito.wtf:443/api/v1/bundles -X POST -H "Content-Type: application/json" -d '
{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getBundleStatuses",
    "params": [
      [
        "892b79ed49138bfb3aa5441f0df6e06ef34f9ee8f3976c15b323605bae0cf51d"
      ]
    ]
}
'
```

### Response

```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 242806119
    },
    "value": [
      {
        "bundle_id": "892b79ed49138bfb3aa5441f0df6e06ef34f9ee8f3976c15b323605bae0cf51d",
        "transactions": [
          "3bC2M9fiACSjkTXZDgeNAuQ4ScTsdKGwR42ytFdhUvikqTmBheUxfsR1fDVsM5ADCMMspuwGkdm1uKbU246x5aE3",
          "8t9hKYEYNbLvNqiSzP96S13XF1C2f1ro271Kdf7bkZ6EpjPLuDff1ywRy4gfaGSTubsM2FeYGDoT64ZwPm1cQUt"
        ],
        "slot": 242804011,
        "confirmation_status": "finalized",
        "err": {
          "Ok": null
        }
      }
    ]
  },
  "id": 1
}
```
