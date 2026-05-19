## 0.45.1

- Update `libp2p-swarm-derive` to version `0.35.0`, see [PR XXXX]

[PR XXXX]: https://github.com/libp2p/rust-libp2p/pull/XXXX

## 0.45.0

- Implement refactored `Transport`.
  See [PR 4568]
- Move `address_translation` into swarm and into `libp2p-identify`.
  See [PR 4568]

[PR 4568]: https://github.com/libp2p/rust-libp2p/pull/4568

## 0.44.3
- Optimize internal connection `fn poll`. New implementation now scales much better with number of listen protocols active.
  No changes to public API introduced.
  See [PR 5026](https://github.com/libp2p/rust-libp2p/pull/5026)
- Add peer_id to `FromSwarm::ListenFailure`.
  See [PR 4818](https://github.com/libp2p/rust-libp2p/pull/4818).
- Use `web-time` instead of `instant`.
  See [PR 5347](https://github.com/libp2p/rust-libp2p/pull/5347).
- Add `#[track_caller]` on all `spawn` wrappers.
  See [PR 5465](https://github.com/libp2p/rust-libp2p/pull/5465).
- Add ConnectionError to FromSwarm::ConnectionClosed.
  See [PR 5485](https://github.com/libp2p/rust-libp2p/pull/5485).

## 0.44.2

- Allow `NetworkBehaviour`s to share addresses of peers.
  This is enabled via the new `ToSwarm::NewExternalAddrOfPeer` event.
  The address is broadcast to all behaviours via `FromSwarm::NewExternalAddrOfPeer`.
  Protocols that want to collect these addresses can use the new `PeerAddresses` utility.
  See [PR 4371](https://github.com/libp2p/rust-libp2p/pull/4371).

## 0.44.1

- Implement `Clone` & `Copy` for `FromSwarm.
  This makes it easier to forward these events when wrapping other behaviours.
  See [PR 4825](https://github.com/libp2p/rust-libp2p/pull/4825).

## 0.44.0

- Add `#[non_exhaustive]` to `FromSwarm`, `ToSwarm`, `SwarmEvent`, `ConnectionHandlerEvent`, `ConnectionEvent`.
  See [PR 4581](https://github.com/libp2p/rust-libp2p/pull/4581).
- Remove `handler` field from `ConnectionClosed`.
  If you need to transfer state from a `ConnectionHandler` to its `NetworkBehaviour` when a connection closes, use `ConnectionHandler::poll_close`.
  See [PR 4076](https://github.com/libp2p/rust-libp2p/pull/4076).
- Remove deprecated `PollParameters` from `NetworkBehaviour::poll` function.
  See [PR 4490](https://github.com/libp2p/rust-libp2p/pull/4490).
- Remove deprecated `ConnectionHandlerEvent::Close` and `ConnectionHandler::Error`.
  `ConnectionHandler`s should not close connections directly as the connection might still be in use by other handlers.
  See [PR 4755](https://github.com/libp2p/rust-libp2p/pull/4755).
- Add `PeerCondition::DisconnectedAndNotDialing` variant, combining pre-existing conditions.
  This is the new default.
  A new dialing attempt is iniated _only if_ the peer is both considered disconnected and there is currently no ongoing dialing attempt.
  See [PR 4225](https://github.com/libp2p/rust-libp2p/pull/4225).
- Remove deprecated `keep_alive_timeout` in `OneShotHandlerConfig`.
  See [PR 4677](https://github.com/libp2p/rust-libp2p/pull/4677).
- Don't close entire connection upon `DialUpgradeError`s within `OneShotHandler`.
  Instead, the error is reported as `Err(e)` via `ConnectionHandler::ToBehaviour`.
  See [PR 4715](https://github.com/libp2p/rust-libp2p/pull/4715).
- Log `PeerId` of `Swarm` even when constructed with new `SwarmBuilder`.
  See [PR 4671](https://github.com/libp2p/rust-libp2p/pull/4671).
- Add `SwarmEvent::{NewExternalAddrCandidate,ExternalAddrConfirmed,ExternalAddrExpired}` variants.
  See [PR 4721](https://github.com/libp2p/rust-libp2p/pull/4721).
- Remove deprecated symbols.
  See [PR 4737](https://github.com/libp2p/rust-libp2p/pull/4737).

## 0.43.7

- Deprecate `ConnectionHandlerEvent::Close`.
  See [issue 3591](https://github.com/libp2p/rust-libp2p/issues/3591) for details.
  See [PR 4714](https://github.com/libp2p/rust-libp2p/pull/4714).

## 0.43.6

- Deprecate `libp2p::swarm::SwarmBuilder`.
  Most users should use `libp2p::SwarmBuilder`.
  In some special cases, users may need to use `Swarm::new` and `Config` instead of the new `libp2p::SwarmBuilder`.
  See [PR 4120].
- Make the `Debug` implementation of `StreamProtocol` more concise.
  See [PR 4631](https://github.com/libp2p/rust-libp2p/pull/4631).
- Fix overflow in `KeepAlive` computation that could occur panic at `Delay::new` if `SwarmBuilder::idle_connection_timeout` is configured too large.
  See [PR 4644](https://github.com/libp2p/rust-libp2p/pull/4644).
- Deprecate `KeepAlive::Until`.
  Individual protocols should not keep connections alive for longer than necessary.
  Users should use `swarm::Config::idle_connection_timeout` instead.
  See [PR 4656](https://github.com/libp2p/rust-libp2p/pull/4656).
- Deprecate `keep_alive_timeout` in `OneShotHandlerConfig`.
  See [PR 4680](https://github.com/libp2p/rust-libp2p/pull/4680).

[PR 4120]: https://github.com/libp2p/rust-libp2p/pull/4120

## 0.43.5

- Fix overflow in `KeepAlive` computation that could occur if `SwarmBuilder::idle_connection_timeout` is configured with `u64::MAX`.
  See [PR 4559](https://github.com/libp2p/rust-libp2p/pull/4559).

## 0.43.4

- Implement `Debug` for event structs.
  See [PR 4426].

- Improve error message when `DialPeerCondition` prevents a dial.
  See [PR 4409].

- Introduce `SwarmBuilder::idle_conncetion_timeout` and deprecate `keep_alive::Behaviour` as a result.
  See [PR 4161].

[PR 4426]: https://github.com/libp2p/rust-libp2p/pull/4426
[PR 4409]: https://github.com/libp2p/rust-libp2p/pull/4409
[PR 4161]: https://github.com/libp2p/rust-libp2p/pull/4161

## 0.43.3

- Implement `Display` for `ConnectionId`.
  See [PR 4278].

[PR 4278]: https://github.com/libp2p/rust-libp2p/pull/4278

## 0.43.2
- Display the cause of a `ListenError::Denied`.
  See [PR 4232]

[PR 4232]: https://github.com/libp2p/rust-libp2p/pull/4158

## 0.43.1

- Do not announce external address candidate before address translation, unless translation does not apply.
  This will prevent ephemeral TCP addresses being announced as external address candidates.
  See [PR 4158].

[PR 4158]: https://github.com/libp2p/rust-libp2p/pull/4158

## 0.43.0

- Allow `NetworkBehaviours` to create and remove listeners.
  See [PR 3292].

- Raise MSRV to 1.65.
  See [PR 3715].

- Introduce `StreamProtocol` type.
  This type enforces invariants on protocol names, such as leading forward slashes and correct UTF8 encoding.
  See [PR 3746].

- Return a bool from `ExternalAddresses::on_swarm_event` and `ListenAddresses::on_swarm_event` indicating whether any state was changed.
  See [PR 3865].

- Remove deprecated banning API from `Swarm`.
  Users should migrate to `libp2p::allow_block_list`.
  See [PR 3886].

- Remove `ConnectionHandlerUpgrErr::Timer` variant.
  This variant was never constructed and thus dead code.
  See [PR 3605].

- Remove deprecated `IntoConnectionHandler` and all its implementations.
  This also removes the `NetworkBehaviour::new_handler` and `NetworkBehaviour::addresses_of_peer` methods.
  See changelog for `0.42` on how to migrate.
  See [PR 3884].

- Remove `ConnectionHandlerUpgrErr::Timer` variant.
  This variant was never constructed and thus dead code.
  See [PR 3605].

- Flatten `ConnectionHandlerUpgrErr` and rename to `StreamUpgradeError`.
  See [PR 3882].

- Remove deprecated `ConnectionLimits`.
  Users should migrate to `libp2p::connection_limits::Behaviour`.
  See [PR 3885].

- Allow `ConnectionHandler`s to report and learn about the supported protocols on a connection.
  The newly introduced API elements are:
  - `ConnectionHandlerEvent::ReportRemoteProtocols`
  - `ConnectionEvent::LocalProtocolsChange`
  - `ConnectionEvent::RemoteProtocolsChange`

  See [PR 3651].

- Deprecate the `NegotiatedSubstream` type and replace it with `Stream`.
  See [PR 3912].

- Rename `NetworkBehaviour::OutEvent` to `NetworkBehaviour::ToSwarm`, `ConnectionHandler::InEvent` to `ConnectionHandler::FromBehaviour`, `ConnectionHandler::OutEvent` to `ConnectionHandler::ToBehaviour`. See [PR 3848].

- Remove deprecated `NetworkBehaviourAction` type.
  See [PR 3919].

- Expose `ConnectionId` on `SwarmEvent::{ConnectionEstablished,ConnectionClosed,IncomingConnection,IncomingConnectionError,OutgoingConnectionError,Dialing}`.
  Also emit `SwarmEvent::Dialing` for dials with unknown `PeerId`.
  See [PR 3927].

- Rename `ConnectionHandlerEvent::Custom` to `ConnectionHandlerEvent::NotifyBehaviour`. See [PR 3955].

- Remove `DialError::InvalidPeerId` variant. With the move to `multiaddr` `v0.18.0` peer IDs in `/p2p` are type safe and thus usage of the contained peer ID can not result in a parsing error.
  See [PR 4037].

- Remove deprecated items. See [PR 3956].

- Add ability to `downcast_ref` ConnectionDenied errors. See [PR 4020].

[PR 3292]: https://github.com/libp2p/rust-libp2p/pull/3292
[PR 3605]: https://github.com/libp2p/rust-libp2p/pull/3605
[PR 3651]: https://github.com/libp2p/rust-libp2p/pull/3651
[PR 3715]: https://github.com/libp2p/rust-libp2p/pull/3715
[PR 3746]: https://github.com/libp2p/rust-libp2p/pull/3746
[PR 3848]: https://github.com/libp2p/rust-libp2p/pull/3848
[PR 3865]: https://github.com/libp2p/rust-libp2p/pull/3865
[PR 3882]: https://github.com/libp2p/rust-libp2p/pull/3882
[PR 3884]: https://github.com/libp2p/rust-libp2p/pull/3884
[PR 3885]: https://github.com/libp2p/rust-libp2p/pull/3885
[PR 3886]: https://github.com/libp2p/rust-libp2p/pull/3886
[PR 3912]: https://github.com/libp2p/rust-libp2p/pull/3912
[PR 3919]: https://github.com/libp2p/rust-libp2p/pull/3919
[PR 3927]: https://github.com/libp2p/rust-libp2p/pull/3927
[PR 3955]: https://github.com/libp2p/rust-libp2p/pull/3955
[PR 3956]: https://github.com/libp2p/rust-libp2p/pull/3956
[PR 4020]: https://github.com/libp2p/rust-libp2p/pull/4020
[PR 4037]: https://github.com/libp2p/rust-libp2p/pull/4037

## 0.42.2

- Add `ConnectionEvent::{is_outbound,is_inbound}`. See [PR 3625].

[PR 3625]: https://github.com/libp2p/rust-libp2p/pull/3625

## 0.42.1

- Deprecate `ConnectionLimits` in favor of `libp2p::connection_limits`.
  See [PR 3386].

- Introduce `ConnectionId::new_unchecked` to allow for more sophisticated, manual tests of `NetworkBehaviour`.
  See [PR 3652].

- Deprecate `Swarm::ban_peer_id` in favor of the new `libp2p::allow_block_list` module.
  See [PR 3590].

- Rename `NetworkBehaviourAction` to `ToSwarm`.
  A deprecated type-alias is provided to ease the transition.
  The new name is meant to better indicate the message-passing relationship between `Swarm` and `NetworkBehaviour`.
  See [PR 3658].

[PR 3386]: https://github.com/libp2p/rust-libp2p/pull/3386
[PR 3652]: https://github.com/libp2p/rust-libp2p/pull/3652
[PR 3590]: https://github.com/libp2p/rust-libp2p/pull/3590
[PR 3658]: https://github.com/libp2p/rust-libp2p/pull/3658

## 0.42.0

- Allow `NetworkBehaviour`s to manage connections.
  We deprecate `NetworkBehaviour::new_handler` and `NetworkBehaviour::addresses_of_peer` in favor of four new callbacks:

  - `NetworkBehaviour::handle_pending_inbound_connection`
  - `NetworkBehaviour::handle_pending_outbound_connection`
  - `NetworkBehaviour::handle_established_inbound_connection`
  - `NetworkBehaviour::handle_established_outbound_connection`

  Please note that due to [limitations](https://github.com/rust-lang/rust/issues/98990) in the Rust compiler, _implementations_ of `new_handler` and `addresses_of_peer` are not flagged as deprecated.
  Nevertheless, they will be removed in the future.

  All four are fallible and returning an error from any of them will abort the given connection.
  This allows you to create dedicated `NetworkBehaviour`s that only concern themselves with managing connections.
  For example:
  - checking the `PeerId` of a newly established connection against an allow/block list
  - only allowing X connection upgrades at any one time
  - denying incoming or outgoing connections from a certain IP range
  - only allowing N connections to or from the same peer

  See [PR 3254].

- Remove `handler` field from `NetworkBehaviourAction::Dial`.
  Instead of constructing the handler early, you can now access the `ConnectionId` of the future connection on `DialOpts`.
  `ConnectionId`s are `Copy` and will be used throughout the entire lifetime of the connection to report events.
  This allows you to send events to a very specific connection, much like you previously could directly set state in the handler.

  Removing the `handler` field also reduces the type parameters of `NetworkBehaviourAction` from three to two.
  The third one used to be defaulted to the `InEvent` of the `ConnectionHandler`.
  You now have to manually specify that where you previously had to specify the `ConnectionHandler`.
  This very likely will trigger **convoluted compile errors** about traits not being implemented.

  Within `NetworkBehaviourAction::poll`, the easiest way to migrate is to do this (in the example of `libp2p-floodsub`):
  ```diff
  --- a/protocols/floodsub/src/layer.rs
  +++ b/protocols/floodsub/src/layer.rs
  @@ -472,7 +465,7 @@ impl NetworkBehaviour for Floodsub {
       &mut self,
       _: &mut Context<'_>,
       _: &mut impl PollParameters,
  -    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
  +    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, THandlerInEvent<Self>>> {
  ```

  In other words:

  |Search|Replace|
    |---|---|
  |`NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>`|`NetworkBehaviourAction<Self::OutEvent, THandlerInEvent<Self>>`|

  If you reference `NetworkBehaviourAction` somewhere else as well,
  you may have to fill in the type of `ConnectionHandler::InEvent` manually as the 2nd parameter.

  See [PR 3328].

- Update to `libp2p-core` `v0.39.0`.

- Removed deprecated Swarm constructors. For transition notes see [0.41.0](#0.41.0). See [PR 3170].

- Deprecate functions on `PollParameters` in preparation for `PollParameters` to be removed entirely eventually. See [PR 3153].

- Add `estblished_in` to `SwarmEvent::ConnectionEstablished`. See [PR 3134].

- Remove deprecated `inject_*` methods from `NetworkBehaviour` and `ConnectionHandler`.
  Make the implementation of `on_swarm_event` and `on_connection_handler_event`
  both mandatory. See [PR 3264] and [PR 3364].

- Update to `libp2p-swarm-derive` `v0.32.0`.

- Replace `SwarmBuilder::connection_event_buffer_size` with `SwarmBuilder::per_connection_event_buffer_size` .
  The configured value now applies _per_ connection.
  The default values remains 7.
  If you have previously set `connection_event_buffer_size` you should re-evaluate what a good size for a _per connection_ buffer is.
  See [PR 3188].

- Remove `DialError::ConnectionIo` variant.
  This was never constructed.
  See [PR 3374].

- Introduce `ListenError` and use it within `SwarmEvent::IncomingConnectionError`.
  See [PR 3375].

- Remove `PendingConnectionError`, `PendingInboundConnectionError` and `PendingOutboundConnectionError` from the public API.
  They are no longer referenced anywhere with the addition of `ListenError`.
  See [PR 3497].

- Remove `ConnectionId::new`. Manually creating `ConnectionId`s is now unsupported. See [PR 3327].

- Deprecate methods `Swarm::with_executor`, `Swarm::with_*_executor`, `Swarm::without_executor`.
  Introduce similar methods in `SwarmBuilder`. See [PR 3588].

- Gracefully disable oneshot handler on dial upgrade errors. See [PR 3577].

[PR 3364]: https://github.com/libp2p/rust-libp2p/pull/3364
[PR 3170]: https://github.com/libp2p/rust-libp2p/pull/3170
[PR 3134]: https://github.com/libp2p/rust-libp2p/pull/3134
[PR 3153]: https://github.com/libp2p/rust-libp2p/pull/3153
[PR 3264]: https://github.com/libp2p/rust-libp2p/pull/3264
[PR 3272]: https://github.com/libp2p/rust-libp2p/pull/3272
[PR 3327]: https://github.com/libp2p/rust-libp2p/pull/3327
[PR 3328]: https://github.com/libp2p/rust-libp2p/pull/3328
[PR 3188]: https://github.com/libp2p/rust-libp2p/pull/3188
[PR 3377]: https://github.com/libp2p/rust-libp2p/pull/3377
[PR 3373]: https://github.com/libp2p/rust-libp2p/pull/3373
[PR 3374]: https://github.com/libp2p/rust-libp2p/pull/3374
[PR 3375]: https://github.com/libp2p/rust-libp2p/pull/3375
[PR 3254]: https://github.com/libp2p/rust-libp2p/pull/3254
[PR 3497]: https://github.com/libp2p/rust-libp2p/pull/3497
[PR 3588]: https://github.com/libp2p/rust-libp2p/pull/3588
[PR 3577]: https://github.com/libp2p/rust-libp2p/pull/3577

## 0.41.1

- Update to `libp2p-swarm-derive` `v0.31.0`.

## 0.41.0

- Update to `libp2p-core` `v0.38.0`.

- Add new `on_connection_event` method to `ConnectionHandler` that accepts a `ConnectionEvent` enum and update
  `inject_*` methods to call `on_connection_event` with the respective `ConnectionEvent` variant and deprecate
  `inject_*`.
  To migrate, users should replace the `ConnectionHandler::inject_*` calls with a single
  implementation of `ConnectionHandler::on_connection_event` treating each `ConnectionEvent` variant in
  the same way its corresponding `inject_*` call was treated.
  See [PR 3085].

- Add new `on_behaviour_event` method with the same signature as `inject_event`, make the
  default implementation of `inject_event` call `on_behaviour_event` and deprecate it.
  To migrate, users should replace the `ConnectionHandler::inject_event` call
  with `ConnectionHandler::on_behaviour_event`.
  See [PR 3085].

- Add new `on_swarm_event` method to `NetworkBehaviour` that accepts a `FromSwarm` enum and update
  `inject_*` methods to call `on_swarm_event` with the respective `FromSwarm` variant and deprecate
  `inject_*`.
  To migrate, users should replace the `NetworkBehaviour::inject_*` calls with a single
  implementation of `NetworkBehaviour::on_swarm_event` treating each `FromSwarm` variant in
  the same way its corresponding `inject_*` call was treated.
  See [PR 3011].

- Add new `on_connection_handler_event` method with the same signature as `inject_event`, make the
  default implementation of `inject_event` call `on_connection_handler_event` and deprecate it.
  To migrate, users should replace the `NetworkBehaviour::inject_event` call
  with `NetworkBehaviour::on_connection_handler_event`.
  See [PR 3011].

- Export `NetworkBehaviour` derive as `libp2p_swarm::NetworkBehaviour`.
  This follows the convention of other popular libraries. `serde` for example exports the `Serialize` trait and macro as
  `serde::Serialize`. See [PR 3055].

- Feature-gate `NetworkBehaviour` macro behind `macros` feature flag. See [PR 3055].

- Make executor in Swarm constructor explicit. See [PR 3097].

  Supported executors:
  - Tokio

    Previously
    ```rust
    let swarm = SwarmBuilder::new(transport, behaviour, peer_id)
        .executor(Box::new(|fut| {
                tokio::spawn(fut);
        }))
        .build();
    ```
    Now
    ```rust
    let swarm = Swarm::with_tokio_executor(transport, behaviour, peer_id);
    ```
  - Async Std

    Previously
    ```rust
    let swarm = SwarmBuilder::new(transport, behaviour, peer_id)
        .executor(Box::new(|fut| {
                async_std::task::spawn(fut);
        }))
        .build();
    ```
    Now
    ```rust
    let swarm = Swarm::with_async_std_executor(transport, behaviour, peer_id);
    ```
  - ThreadPool (see [Issue 3107])

    In most cases ThreadPool can be replaced by executors or spawning on the local task.

    Previously
    ```rust
    let swarm = Swarm::new(transport, behaviour, peer_id);
    ```

    Now
    ```rust
    let swarm = Swarm::with_threadpool_executor(transport, behaviour, peer_id);
    ```
  - Without

    Spawns the tasks on the current task, this may result in bad performance so try to use an executor where possible. Previously this was just a fallback when no executor was specified and constructing a `ThreadPool` failed.

    New
    ```rust
    let swarm = Swarm::without_executor(transport, behaviour, peer_id);
    ```

  Deprecated APIs:
  - `Swarm::new`
  - `SwarmBuilder::new`
  - `SwarmBuilder::executor`

- Update `rust-version` to reflect the actual MSRV: 1.62.0. See [PR 3090].

[PR 3085]: https://github.com/libp2p/rust-libp2p/pull/3085
[PR 3011]: https://github.com/libp2p/rust-libp2p/pull/3011
[PR 3055]: https://github.com/libp2p/rust-libp2p/pull/3055
[PR 3097]: https://github.com/libp2p/rust-libp2p/pull/3097
[Issue 3107]: https://github.com/libp2p/rust-libp2p/issues/3107
[PR 3090]: https://github.com/libp2p/rust-libp2p/pull/3090

## 0.40.1

- Bump rand to 0.8 and quickcheck to 1. See [PR 2857].

- Update to `libp2p-core` `v0.37.0`.

- Introduce `libp2p_swarm::keep_alive::ConnectionHandler` in favor of removing `keep_alive` from
  `libp2p_swarm::dummy::ConnectionHandler`. `dummy::ConnectionHandler` now literally does not do anything. In the same
  spirit, introduce `libp2p_swarm::keep_alive::Behaviour` and `libp2p_swarm::dummy::Behaviour`. See [PR 2859].

[PR 2857]: https://github.com/libp2p/rust-libp2p/pull/2857
[PR 2859]: https://github.com/libp2p/rust-libp2p/pull/2859/

- Pass actual `PeerId` of dial to `NetworkBehaviour::inject_dial_failure` on `DialError::ConnectionLimit`. See [PR 2928].

[PR 2928]: https://github.com/libp2p/rust-libp2p/pull/2928


## 0.39.0

- Remove deprecated `NetworkBehaviourEventProcess`. See [libp2p-swarm v0.38.0 changelog entry] for
  migration path.

- Update to `libp2p-core` `v0.36.0`.

- Enforce backpressure on incoming streams via `StreamMuxer` interface. In case we hit the configured limit of maximum
  number of inbound streams, we will stop polling the `StreamMuxer` for new inbound streams. Depending on the muxer
  implementation in use, this may lead to instant dropping of inbound streams. See [PR 2861].

[libp2p-swarm v0.38.0 changelog entry]: https://github.com/libp2p/rust-libp2p/blob/master/swarm/CHANGELOG.md#0380
[PR 2861]: https://github.com/libp2p/rust-libp2p/pull/2861/

## 0.38.0

- Deprecate `NetworkBehaviourEventProcess`. When deriving `NetworkBehaviour` on a custom `struct` users
  should either bring their own `OutEvent` via `#[behaviour(out_event = "MyBehaviourEvent")]` or,
  when not specified, have the derive macro generate one for the user.

  See [`NetworkBehaviour`
  documentation](https://docs.rs/libp2p/latest/libp2p/swarm/trait.NetworkBehaviour.html) and [PR
  2784] for details.

  Previously

  ``` rust
  #[derive(NetworkBehaviour)]
  #[behaviour(event_process = true)]
  struct MyBehaviour {
      gossipsub: Gossipsub,
      mdns: Mdns,
  }

  impl NetworkBehaviourEventProcess<Gossipsub> for MyBehaviour {
      fn inject_event(&mut self, message: GossipsubEvent) {
        todo!("Handle event")
      }
  }

  impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
      fn inject_event(&mut self, message: MdnsEvent) {
        todo!("Handle event")
      }
  }
  ```

  Now

  ``` rust
  #[derive(NetworkBehaviour)]
  #[behaviour(out_event = "MyBehaviourEvent")]
  struct MyBehaviour {
      gossipsub: Gossipsub,
      mdns: Mdns,
  }

  enum MyBehaviourEvent {
      Gossipsub(GossipsubEvent),
      Mdns(MdnsEvent),
  }

  impl From<GossipsubEvent> for MyBehaviourEvent {
      fn from(event: GossipsubEvent) -> Self {
          MyBehaviourEvent::Gossipsub(event)
      }
  }

  impl From<MdnsEvent> for MyBehaviourEvent {
      fn from(event: MdnsEvent) -> Self {
          MyBehaviourEvent::Mdns(event)
      }
  }

  match swarm.next().await.unwrap() {
    SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(event)) => {
      todo!("Handle event")
    }
    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(event)) => {
      todo!("Handle event")
    }
  }
  ```

- When deriving `NetworkBehaviour` on a custom `struct` where the user does not specify their own
  `OutEvent` via `#[behaviour(out_event = "MyBehaviourEvent")]` and where the user does not enable
  `#[behaviour(event_process = true)]`, then the derive macro generates an `OutEvent` definition for
  the user.

  See [`NetworkBehaviour`
  documentation](https://docs.rs/libp2p/latest/libp2p/swarm/trait.NetworkBehaviour.html) and [PR
  2792] for details.

- Update dial address concurrency factor to `8`, thus dialing up to 8 addresses concurrently for a single connection attempt. See `Swarm::dial_concurrency_factor` and [PR 2741].

- Update to `libp2p-core` `v0.35.0`.

[PR 2741]: https://github.com/libp2p/rust-libp2p/pull/2741/
[PR 2784]: https://github.com/libp2p/rust-libp2p/pull/2784
[PR 2792]: https://github.com/libp2p/rust-libp2p/pull/2792

## 0.37.0

- Update to `libp2p-core` `v0.34.0`.

- Extend log message when exceeding inbound negotiating streams with peer ID and limit. See [PR 2716].

- Remove `connection::ListenersStream` and poll the `Transport` directly. See [PR 2652].

[PR 2716]: https://github.com/libp2p/rust-libp2p/pull/2716/
[PR 2652]: https://github.com/libp2p/rust-libp2p/pull/2652

## 0.36.1

- Limit negotiating inbound substreams per connection. See [PR 2697].

[PR 2697]: https://github.com/libp2p/rust-libp2p/pull/2697

## 0.36.0

- Don't require `Transport` to be `Clone`. See [PR 2529].

- Update to `libp2p-core` `v0.33.0`.

- Make `behaviour::either` module private. See [PR 2610]

- Rename `IncomingInfo::to_connected_point` to `IncomingInfo::create_connected_point`. See [PR 2620].

- Rename `TProtoHandler` to `TConnectionHandler`, `ToggleProtoHandler` to `ToggleConnectionHandler`, `ToggleIntoProtoHandler` to `ToggleIntoConnectionHandler`. See [PR 2640].

[PR 2529]: https://github.com/libp2p/rust-libp2p/pull/2529
[PR 2610]: https://github.com/libp2p/rust-libp2p/pull/2610
[PR 2620]: https://github.com/libp2p/rust-libp2p/pull/2620
[PR 2640]: https://github.com/libp2p/rust-libp2p/pull/2640

## 0.35.0

- Add impl `IntoIterator` for `MultiHandler`. See [PR 2572].
- Remove `Send` bound from `NetworkBehaviour`. See [PR 2535].

[PR 2572]: https://github.com/libp2p/rust-libp2p/pull/2572/
[PR 2535]: https://github.com/libp2p/rust-libp2p/pull/2535/

## 0.34.0 [2022-02-22]

- Rename `ProtocolsHandler` to `ConnectionHandler`. Upgrade should be as simple as renaming all
  occurrences of `ProtocolsHandler` to `ConnectionHandler` with your favorite text manipulation tool
  across your codebase. See [PR 2527].

- Fold `libp2p-core`'s `Network` into `Swarm`. See [PR 2492].

- Update to `libp2p-core` `v0.32.0`.

- Disconnect pending connections with `Swarm::disconnect`. See [PR 2517].

- Report aborted connections via `SwarmEvent::OutgoingConnectionError`. See [PR 2517].

[PR 2492]: https://github.com/libp2p/rust-libp2p/pull/2492
[PR 2517]: https://github.com/libp2p/rust-libp2p/pull/2517
[PR 2527]: https://github.com/libp2p/rust-libp2p/pull/2527

## 0.33.0 [2022-01-27]

- Patch reporting on banned peers and their non-banned and banned connections (see [PR 2350]).

- Update dependencies.

- Migrate to Rust edition 2021 (see [PR 2339]).

- Update `Connection::address` on `inject_address_change` (see [PR 2362]).

- Move `swarm::Toggle` to `swarm::behaviour::Toggle` (see [PR 2375]).

- Add `Swarm::connected_peers` (see [PR 2378]).

- Implement `swarm::NetworkBehaviour` on `either::Either` (see [PR 2370]).

- Allow overriding _dial concurrency factor_ per dial via
  `DialOpts::override_dial_concurrency_factor`. See [PR 2404].

- Report negotiated and expected `PeerId` as well as remote address in
  `DialError::WrongPeerId` (see [PR 2428]).

- Allow overriding role when dialing through `override_role` option on
  `DialOpts`. This option is needed for NAT and firewall hole punching. See [PR
  2363].

- Merge NetworkBehaviour's inject_\* paired methods (see PR 2445).

[PR 2339]: https://github.com/libp2p/rust-libp2p/pull/2339
[PR 2350]: https://github.com/libp2p/rust-libp2p/pull/2350
[PR 2362]: https://github.com/libp2p/rust-libp2p/pull/2362
[PR 2370]: https://github.com/libp2p/rust-libp2p/pull/2370
[PR 2375]: https://github.com/libp2p/rust-libp2p/pull/2375
[PR 2378]: https://github.com/libp2p/rust-libp2p/pull/2378
[PR 2404]: https://github.com/libp2p/rust-libp2p/pull/2404
[PR 2428]: https://github.com/libp2p/rust-libp2p/pull/2428
[PR 2363]: https://github.com/libp2p/rust-libp2p/pull/2363
[PR 2445]: https://github.com/libp2p/rust-libp2p/pull/2445

## 0.32.0 [2021-11-16]

- Use `instant` and `futures-timer` instead of `wasm-timer` (see [PR 2245]).

- Enable advanced dialing requests both on `Swarm::dial` and via
  `NetworkBehaviourAction::Dial`. Users can now trigger a dial with a specific
  set of addresses, optionally extended via
  `NetworkBehaviour::addresses_of_peer`.

   Changes required to maintain status quo:

  - Previously `swarm.dial(peer_id)`
     now `swarm.dial(DialOpts::peer_id(peer_id).build())`
     or `swarm.dial(peer_id)` given that `DialOpts` implements `From<PeerId>`.

  - Previously `swarm.dial_addr(addr)`
     now `swarm.dial(DialOpts::unknown_peer_id().address(addr).build())`
     or `swarm.dial(addr)` given that `DialOpts` implements `From<Multiaddr>`.

  - Previously `NetworkBehaviourAction::DialPeer { peer_id, condition, handler }`
     now

     ```rust
     NetworkBehaviourAction::Dial {
       opts: DialOpts::peer_id(peer_id)
         .condition(condition)
         .build(),
       handler,
     }
     ```

  - Previously `NetworkBehaviourAction::DialAddress { address, handler }`
     now

     ```rust
     NetworkBehaviourAction::Dial {
       opts: DialOpts::unknown_peer_id()
         .address(address)
         .build(),
       handler,
     }
     ```

   See [PR 2317].

[PR 2245]: https://github.com/libp2p/rust-libp2p/pull/2245
[PR 2317]: https://github.com/libp2p/rust-libp2p/pull/2317

## 0.31.0 [2021-11-01]

- Make default features of `libp2p-core` optional.
  [PR 2181](https://github.com/libp2p/rust-libp2p/pull/2181)

- Update dependencies.

- Provide default implementations for all functions of `NetworkBehaviour`,
  except for `new_handler`, `inject_event` and `poll`.
  This should make it easier to create new implementations. See [PR 2150].

- Remove `Swarm` type alias and rename `ExpandedSwarm` to `Swarm`. Reduce direct
  trait parameters on `Swarm` (previously `ExpandedSwarm`), deriving parameters
  through associated types on `TBehaviour`. See [PR 2182].

- Require `ProtocolsHandler::{InEvent,OutEvent,Error}` to implement `Debug` (see
  [PR 2183]).

- Implement `ProtocolsHandler` on `either::Either`representing either of two
  `ProtocolsHandler` implementations (see [PR 2192]).

- Require implementation to provide handler in
  `NetworkBehaviourAction::DialPeer` and `NetworkBehaviourAction::DialAddress`.
  Note that the handler is returned to the `NetworkBehaviour` on connection
  failure and connection closing. Thus it can be used to carry state, which
  otherwise would have to be tracked in the `NetworkBehaviour` itself. E.g. a
  message destined to an unconnected peer can be included in the handler, and
  thus directly send on connection success or extracted by the
  `NetworkBehaviour` on connection failure (see [PR 2191]).

- Include handler in `NetworkBehaviour::inject_dial_failure`,
  `NetworkBehaviour::inject_connection_closed`,
  `NetworkBehaviour::inject_listen_failure` (see [PR 2191]).

- Include error in `NetworkBehaviour::inject_dial_failure` and call
  `NetworkBehaviour::inject_dial_failure` on `DialPeerCondition` evaluating to
  false. To emulate the previous behaviour, return early within
  `inject_dial_failure` on `DialError::DialPeerConditionFalse`. See [PR 2191].

- Make `NetworkBehaviourAction` generic over `NetworkBehaviour::OutEvent` and
  `NetworkBehaviour::ProtocolsHandler`. In most cases, change your generic type
  parameters to `NetworkBehaviourAction<Self::OutEvent,
  Self::ProtocolsHandler>`. See [PR 2191].

- Return `bool` instead of `Result<(), ()>` for `Swarm::remove_listener`(see
  [PR 2261]).

- Concurrently dial address candidates within a single dial attempt (see [PR 2248]) configured via
  `Swarm::dial_concurrency_factor`.

  - On success of a single address, report errors of the thus far failed dials via
    `SwarmEvent::ConnectionEstablished::outgoing`.

  - On failure of all addresses, report errors via the new `SwarmEvent::OutgoingConnectionError`.

  - Remove `SwarmEvent::UnreachableAddr` and `SwarmEvent::UnknownPeerUnreachableAddr` event.

  - In `NetworkBehaviour::inject_connection_established` provide errors of all thus far failed addresses.

  - On unknown peer dial failures, call `NetworkBehaviour::inject_dial_failure` with a peer ID of `None`.

  - Remove `NetworkBehaviour::inject_addr_reach_failure`. Information is now provided via
    `NetworkBehaviour::inject_connection_established` and `NetworkBehaviour::inject_dial_failure`.

[PR 2150]: https://github.com/libp2p/rust-libp2p/pull/2150
[PR 2182]: https://github.com/libp2p/rust-libp2p/pull/2182
[PR 2183]: https://github.com/libp2p/rust-libp2p/pull/2183
[PR 2192]: https://github.com/libp2p/rust-libp2p/pull/2192
[PR 2191]: https://github.com/libp2p/rust-libp2p/pull/2191
[PR 2248]: https://github.com/libp2p/rust-libp2p/pull/2248
[PR 2261]: https://github.com/libp2p/rust-libp2p/pull/2261

## 0.30.0 [2021-07-12]

- Update dependencies.

- Drive `ExpandedSwarm` via `Stream` trait only.

  - Change `Stream` implementation of `ExpandedSwarm` to return all
    `SwarmEvents` instead of only the `NetworkBehaviour`'s events.

  - Remove `ExpandedSwarm::next_event`. Users can use `<ExpandedSwarm as
    StreamExt>::next` instead.

  - Remove `ExpandedSwarm::next`. Users can use `<ExpandedSwarm as
    StreamExt>::filter_map` instead.

  See [PR 2100] for details.

- Add `ExpandedSwarm::disconnect_peer_id` and
  `NetworkBehaviourAction::CloseConnection` to close connections to a specific
  peer via an `ExpandedSwarm` or `NetworkBehaviour`. See [PR 2110] for details.

- Expose the `ListenerId` in `SwarmEvent`s that are associated with a listener.

  See [PR 2123] for details.

[PR 2100]: https://github.com/libp2p/rust-libp2p/pull/2100
[PR 2110]: https://github.com/libp2p/rust-libp2p/pull/2110/
[PR 2123]: https://github.com/libp2p/rust-libp2p/pull/2123

## 0.29.0 [2021-04-13]

- Remove `Deref` and `DerefMut` implementations previously dereferencing to the
  `NetworkBehaviour` on `Swarm`. Instead one can access the `NetworkBehaviour`
  via `Swarm::behaviour` and `Swarm::behaviour_mut`. Methods on `Swarm` can now
  be accessed directly, e.g. via `my_swarm.local_peer_id()`. You may use the
  command below to transform fully qualified method calls on `Swarm` to simple
  method calls [PR 1995](https://github.com/libp2p/rust-libp2p/pull/1995).

  ``` bash
  # Go from e.g. `Swarm::local_peer_id(&my_swarm)` to `my_swarm.local_peer_id()`.
  grep -RiIl --include \*.rs --exclude-dir target . --exclude-dir .git | xargs sed -i "s/\(libp2p::\)*Swarm::\([a-z_]*\)(&mut \([a-z_0-9]*\), /\3.\2(/g"
  ```

- Extend `NetworkBehaviour` callbacks, more concretely introducing new `fn
  inject_new_listener` and `fn inject_expired_external_addr` and have `fn
  inject_{new,expired}_listen_addr` provide a `ListenerId` [PR
  2011](https://github.com/libp2p/rust-libp2p/pull/2011).

## 0.28.0 [2021-03-17]

- New error variant `DialError::InvalidAddress`

- `Swarm::dial_addr()` now returns a `DialError` on error.

- Remove the option for a substream-specific multistream select protocol override.
  The override at this granularity is no longer deemed useful, in particular because
  it can usually not be configured for existing protocols like `libp2p-kad` and others.
  There is a `Swarm`-scoped configuration for this version available since
  [1858](https://github.com/libp2p/rust-libp2p/pull/1858).

## 0.27.2 [2021-02-04]

- Have `ToggleProtoHandler` ignore listen upgrade errors when disabled.
  [PR 1945](https://github.com/libp2p/rust-libp2p/pull/1945/files).

## 0.27.1 [2021-01-27]

- Make `OneShotHandler`s `max_dial_negotiate` limit configurable.
  [PR 1936](https://github.com/libp2p/rust-libp2p/pull/1936).

- Fix handling of DialPeerCondition::Always.
  [PR 1937](https://github.com/libp2p/rust-libp2p/pull/1937).

## 0.27.0 [2021-01-12]

- Update dependencies.

## 0.26.0 [2020-12-17]

- Update `libp2p-core`.

- Remove `NotifyHandler::All` thus removing the requirement for events send from
  a `NetworkBehaviour` to a `ProtocolsHandler` to be `Clone`.
  [PR 1880](https://github.com/libp2p/rust-libp2p/pull/1880).

## 0.25.1 [2020-11-26]

- Add `ExpandedSwarm::is_connected`.
  [PR 1862](https://github.com/libp2p/rust-libp2p/pull/1862).

## 0.25.0 [2020-11-25]

- Permit a configuration override for the substream upgrade protocol
  to use for all (outbound) substreams.
  [PR 1858](https://github.com/libp2p/rust-libp2p/pull/1858).

- Changed parameters for connection limits from `usize` to `u32`.
  Connection limits are now configured via `SwarmBuilder::connection_limits()`.

- Update `libp2p-core`.

- Expose configurable scores for external addresses, as well as
  the ability to remove them and to add addresses that are
  retained "forever" (or until explicitly removed).
  [PR 1842](https://github.com/libp2p/rust-libp2p/pull/1842).

## 0.24.0 [2020-11-09]

- Update dependencies.

## 0.23.0 [2020-10-16]

- Require a `Boxed` transport to be given to the `Swarm`
  or `SwarmBuilder` to avoid unnecessary double-boxing of
  transports and simplify API bounds.
  [PR 1794](https://github.com/libp2p/rust-libp2p/pull/1794)

- Respect inbound timeouts and upgrade versions in the `MultiHandler`.
  [PR 1786](https://github.com/libp2p/rust-libp2p/pull/1786).

- Instead of iterating each inbound and outbound substream upgrade looking for
  one to make progress, use a `FuturesUnordered` for both pending inbound and
  pending outbound upgrades. As a result only those upgrades are polled that are
  ready to progress.

  Implementors of `InboundUpgrade` and `OutboundUpgrade` need to ensure to wake
  up the underlying task once they are ready to make progress as they won't be
  polled otherwise.

  [PR 1775](https://github.com/libp2p/rust-libp2p/pull/1775)

## 0.22.0 [2020-09-09]

- Bump `libp2p-core` dependency.

- Adds `ProtocolsHandler::InboundOpenInfo` type which mirrors the existing
  `OutboundOpenInfo` type. A value of this type is passed as an extra argument
  to `ProtocolsHandler::inject_fully_negotiated_inbound` and
  `ProtocolsHandler::inject_listen_upgrade_error`.

- `SubstreamProtocol` now has a second type parameter corresponding to
  inbound or outbound information, a value of which is part of `SubstreamProtocol`
  now. Consequently `ProtocolsHandlerEvent::OutboundSubstreamRequest` no longer
  has a separate `info` field.

## 0.21.0 [2020-08-18]

- Add missing delegation calls in some `ProtocolsHandler` wrappers.
See [PR 1710](https://github.com/libp2p/rust-libp2p/pull/1710).

- Add as_ref and as_mut functions to Toggle
[PR 1684](https://github.com/libp2p/rust-libp2p/pull/1684).

- The `cause` of `SwarmEvent::ConnectionClosed` is now an `Option`,
and `None` indicates an active connection close not caused by an
error.

- `DialError::Banned` has been added and is returned from `Swarm::dial`
if the peer is banned, thereby also invoking the `NetworkBehaviour::inject_dial_failure`
callback.

- Update the `libp2p-core` dependency to `0.21`, fixing [1584](https://github.com/libp2p/rust-libp2p/issues/1584).

- Fix connections being kept alive by `OneShotHandler` when not handling any
  requests [PR 1698](https://github.com/libp2p/rust-libp2p/pull/1698).

## 0.20.1 [2020-07-08]

- Documentation updates.

- Ignore addresses returned by `NetworkBehaviour::addresses_of_peer`
that the `Swarm` considers to be listening addresses of the local node. This
avoids futile dialing attempts of a node to itself, which can otherwise
even happen in genuine situations, e.g. after the local node changed
its network identity and a behaviour makes a dialing attempt to a
former identity using the same addresses.

## 0.20.0 [2020-07-01]

- Updated the `libp2p-core` dependency.

- Add `ProtocolsHandler::inject_listen_upgrade_error`, the inbound
analogue of `ProtocolsHandler::inject_dial_upgrade_error`, with an
empty default implementation. No implementation is required to
retain existing behaviour.

- Add `ProtocolsHandler::inject_address_change` and
`NetworkBehaviour::inject_address_change` to notify of a change in
the address of an existing connection.

## 0.19.1 [2020-06-18]

- Bugfix: Fix MultiHandler panicking when empty
  ([PR 1598](https://github.com/libp2p/rust-libp2p/pull/1598)).
