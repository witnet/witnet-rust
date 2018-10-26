# Managers

The __managers__ are actors that are in charge of some state and that offer some functionality to
other actors (which may be managers as well or not).

The way managers offer their functionality is by handling different types of messages that other
actors can send to them and by responding to them (if necessary).

Also, some managers may have __periodic__ tasks in order to update their state or perform some
actions.

When possible, the logic of the manager will not be placed in the actor itself, but in a separated
library in order to decouple the logic as much as possible from the __Actix__ ecosystem.

There can only be one manager actor of each type per system, and they must be registered into the
system registry. This way, any other actor can get the address of the any manager and send messages
to it.

## State

A manager actor is defined as a struct with some (or no) state:

```rust
/// Any manager actor
#[derive(Default)]
pub struct Manager {
    // Whichever state it might need
    state: ManagerState,
}
```

In order to become actors, the managers must implement the `Actor` trait:

```rust
/// Make actor from `Manager`
impl Actor for Manager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;
    
    /// Method that is executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("Manager actor has been started!")
    }
}
```

`Manager` actors require the implementation of the `Default` trait (as well as `Supervised` and
`SystemService` traits) to become a service that can be registered in the system registry.


## Actor creation and registration

The creation of a manager actor is usually performed directly by the `main` process:

```rust
let manager_addr = Manager::default().start();
```

Once the manager actor is started, the `main` process registers the manager into the system registry:

```rust
System::current().registry().set(manager_addr);
```

## API

### Messages

Messages are defined as `structs` that contain some input parameters and that must implement the
`Message` trait.

In the `Message` trait, the `Result` type needs to be specified. This type is the return type of the
function that will handle the message when it arrives:

```rust 
/// Message handled by the manager 
pub struct ManagerMessage {
    /// Parameter 
    pub param: ParamType,
}

impl Message for ManagerMessage {
    type Result = ManagerMessageResult;
}
```

### Incoming messages: Other actors -> Manager

Managers will handle the reception of different messages. Each message handler will have one
or more input parameters and one output.

When a `ManagerMessage` message arrives at the manager actor, it has to be processed by a handler
function. That handler function needs to be defined inside the `Handler<Message>` trait of the
manager. This trait must also define the `Result` type as well, just like it was done in the
implementation of the `Message` trait:

```rust
/// Handler for ManagerMessage message
impl Handler<ManagerMessage> for Manager {
    type Result = ManagerMessageResult;

    fn handle(&mut self, msg: ManagerMessage, _: &mut Context<Self>) -> Self::Result {
        // Do things to handle the message 
    }
}
```

### Outgoing messages: Manager -> Other actors

The way other actors will communicate with the manager is:

1. Get the address of the manager from the registry:
```rust
// Get manager address
let manager_addr = System::current().registry().get::<Manager>();
```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to
send a message to the actor:
```rust
// Example 
manager_addr
    .send(ManagerMessage{param})
    .into_actor(self)
    .then(|res, _act, _ctx| {
        actix::fut::ok(())
    })
    .wait(ctx);
```
