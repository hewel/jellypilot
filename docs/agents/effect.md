## Goal

This project uses Effect. When adding, editing, or refactoring code, prefer Effect’s typed error model instead of scattering raw `try/catch` blocks throughout business logic.

Core rule:

> Avoid raw `try/catch` inside business workflows. Use `Effect.try` / `Effect.tryPromise` at non-Effect API boundaries. Handle errors only where the code can recover, translate, or produce an external response.

This does not mean `try/catch` is banned. It means errors should be explicit, typed, composable, testable, and handled at the right layer.

---

## Principles

### 1. Prefer `Effect.fn` and `Effect.gen` for Effect code

When creating a function that returns an Effect, prefer:

- `Effect.fn("functionName")`
- `Effect.gen(function* () { ... })`
- `yield*` for sequencing Effect values
- `.pipe(...)` for composition

Recommended:

```ts
import { Effect } from "effect"

export const loadUser = Effect.fn("loadUser")(function* (id: string) {
  const user = yield* findUserById(id)
  return user
})
````

Avoid:

```ts
export async function loadUser(id: string) {
  try {
    return await findUserById(id)
  } catch (error) {
    console.error(error)
    throw error
  }
}
```

---

## Error Handling Policy

### 2. Recoverable errors must use the Effect error channel

Business errors, input errors, database errors, third-party service errors, permission errors, and missing resources should be represented as typed errors.

Prefer `Schema.TaggedErrorClass`:

```ts
import { Schema } from "effect"

export class UserNotFound extends Schema.TaggedErrorClass<UserNotFound>()("UserNotFound", {
  userId: Schema.String
}) {}

export class InvalidUserInput extends Schema.TaggedErrorClass<InvalidUserInput>()("InvalidUserInput", {
  field: Schema.String,
  message: Schema.String
}) {}
```

Business functions should expose these errors through Effect:

```ts
import { Effect } from "effect"

export const getUserProfile = Effect.fn("getUserProfile")(function* (userId: string) {
  if (userId.length === 0) {
    return yield* new InvalidUserInput({
      field: "userId",
      message: "userId is required"
    })
  }

  return yield* findUserProfile(userId)
})
```

---

## `try/catch` Decision Table

| Scenario                            | Use raw `try/catch`? | Preferred approach                              |
| ----------------------------------- | -------------------: | ----------------------------------------------- |
| Business logic                      |                   No | Use the Effect error channel                    |
| Synchronous API may throw           |                   No | Wrap with `Effect.try`                          |
| Promise API may reject              |                   No | Wrap with `Effect.tryPromise`                   |
| HTTP / CLI / Worker entrypoint      |            Sometimes | Run Effect and translate errors at the boundary |
| Logging then rethrowing             |                   No | Use Effect logging or boundary-level logging    |
| Resource cleanup                    |                   No | Use `Effect.acquireRelease`, `Scope`, or Layer  |
| Broken invariant / programmer error |       Do not swallow | Expose as defect or fail fast                   |
| Tests                               |           Usually no | Assert on Effect result, error, or Exit         |

---

## How to Refactor Existing `try/catch`

When an agent sees a `try/catch`, apply the following process.

### Step 1: Is it inside business logic?

If yes, prefer replacing it with Effect composition.

Avoid:

```ts
export const createOrder = async (input: CreateOrderInput) => {
  try {
    const user = await getUser(input.userId)
    const order = await insertOrder(user, input)
    return order
  } catch (error) {
    throw new Error("Failed to create order")
  }
}
```

Recommended:

```ts
import { Effect } from "effect"

export const createOrder = Effect.fn("createOrder")(function* (input: CreateOrderInput) {
  const user = yield* getUser(input.userId)
  const order = yield* insertOrder(user, input)
  return order
})
```

---

### Step 2: Is it wrapping a synchronous API that may throw?

Use `Effect.try`.

Recommended:

```ts
import { Effect, Schema } from "effect"

export class JsonParseError extends Schema.TaggedErrorClass<JsonParseError>()("JsonParseError", {
  input: Schema.String,
  cause: Schema.Defect
}) {}

export const parseJson = Effect.fn("parseJson")((input: string) =>
  Effect.try({
    try: () => JSON.parse(input) as unknown,
    catch: (cause) =>
      new JsonParseError({
        input,
        cause
      })
  })
)
```

Avoid duplicating this everywhere:

```ts
try {
  return JSON.parse(input)
} catch {
  return null
}
```

Reason: the knowledge of how JSON parsing errors are represented should live in one place.

---

### Step 3: Is it wrapping a Promise API that may reject?

Use `Effect.tryPromise`.

Recommended:

```ts
import { Effect, Schema } from "effect"

export class PaymentProviderError extends Schema.TaggedErrorClass<PaymentProviderError>()(
  "PaymentProviderError",
  {
    cause: Schema.Defect
  }
) {}

export const chargePayment = Effect.fn("chargePayment")((request: PaymentRequest) =>
  Effect.tryPromise({
    try: () => paymentProvider.charge(request),
    catch: (cause) => new PaymentProviderError({ cause })
  })
)
```

---

### Step 4: Does the `catch` block actually recover?

Only catch where the code can make a meaningful decision.

Recommended:

```ts
const program = createOrder(input).pipe(
  Effect.catchTag("UserNotFound", (error) =>
    Effect.succeed({
      status: 404 as const,
      body: {
        message: "User not found",
        userId: error.userId
      }
    })
  ),
  Effect.catchTag("InvalidUserInput", (error) =>
    Effect.succeed({
      status: 400 as const,
      body: {
        field: error.field,
        message: error.message
      }
    })
  )
)
```

Avoid:

```ts
try {
  return await createOrder(input)
} catch (error) {
  console.error(error)
  throw error
}
```

If the code only logs and rethrows, usually do not catch there. Let the error flow through Effect and log it at the boundary.

---

## Error Categories

### Recoverable errors

Use the Effect error channel for:

* Invalid user input
* JSON or schema parsing failure
* Permission denied
* Resource not found
* Database failure
* Third-party API failure
* Payment failure
* Email sending failure
* File read/write failure
* Queue processing failure
* Business rule violation

Example:

```ts
export class OrderAlreadyPaid extends Schema.TaggedErrorClass<OrderAlreadyPaid>()(
  "OrderAlreadyPaid",
  {
    orderId: Schema.String
  }
) {}
```

---

### Non-recoverable errors / defects

Do not hide these as normal business errors:

* Broken invariants
* Impossible branches
* Wrong internal function usage
* Corrupted internal state
* Missing required dependencies
* Runtime failures caused by unsafe type assertions
* Bugs caused by incorrect assumptions in code

Avoid:

```ts
try {
  return calculateInternalState(state)
} catch {
  return defaultState
}
```

Recommended:

```ts
if (state.kind !== "ready") {
  return yield* Effect.dieMessage("Invariant violated: state must be ready")
}
```

Principle:

> Business failures can be recovered from. Programmer errors should be exposed early.

---

## Resource Management

Avoid using scattered `try/finally` blocks for resource cleanup.

Avoid:

```ts
const connection = await openConnection()

try {
  return await runQuery(connection)
} finally {
  await connection.close()
}
```

Prefer Effect resource management:

```ts
import { Effect } from "effect"

export const useConnection = Effect.acquireRelease(
  Effect.tryPromise({
    try: () => openConnection(),
    catch: (cause) => new DatabaseConnectionError({ cause })
  }),
  (connection) =>
    Effect.promise(() => connection.close())
)
```

Then use it from an Effect workflow:

```ts
export const runDatabaseJob = Effect.fn("runDatabaseJob")(function* () {
  const connection = yield* useConnection
  const result = yield* queryWithConnection(connection)
  return result
})
```

Rules:

* The code that acquires a resource should define how it is released.
* Files, connections, locks, transactions, subscriptions, and timers should be modeled as Effect resources.
* Do not make callers remember to manually write `finally`.
* Prefer `Effect.acquireRelease`, `Scope`, and Layer.

---

## Logging and Tracing

Do not break the error flow just to log.

Avoid:

```ts
try {
  return await service.doWork()
} catch (error) {
  logger.error(error)
  throw error
}
```

Prefer attaching logging, spans, or diagnostics through Effect:

```ts
export const doWork = Effect.fn("doWork")(
  function* () {
    const result = yield* serviceWork()
    return result
  },
  Effect.withSpan("doWork")
)
```

When logging a known recoverable error, do it at a meaningful recovery or boundary point:

```ts
const handled = doWork.pipe(
  Effect.catchTag("RemoteServiceError", (error) =>
    Effect.gen(function* () {
      yield* Effect.logError("Remote service failed", error)
      return yield* error
    })
  )
)
```

Do not swallow the error unless the business logic explicitly says it is safe.

---

## Boundary Rules

Run Effect programs at system boundaries.

Common boundaries:

* HTTP route handler
* CLI `main`
* Worker job handler
* Queue consumer
* Cron task
* Legacy callback
* Framework hook

Boundary code may:

* Run the Effect
* Convert typed errors to HTTP responses
* Set process exit codes
* Trigger retry logic
* Write to a dead-letter queue
* Log errors
* Return user-facing messages

Domain services should not know about HTTP status codes.

Avoid in domain code:

```ts
return {
  status: 404,
  body: "User not found"
}
```

Recommended in domain code:

```ts
return yield* new UserNotFound({ userId })
```

Then translate at the HTTP boundary:

```ts
const response = getUserProfile(userId).pipe(
  Effect.catchTag("UserNotFound", (error) =>
    Effect.succeed({
      status: 404 as const,
      body: {
        message: "User not found",
        userId: error.userId
      }
    })
  )
)
```

---

## Standard Refactoring Procedure for Agents

When modifying code that contains `try/catch`, follow this procedure:

1. Identify the API calls inside the `try` block.
2. Determine whether each API:

   * throws synchronously,
   * rejects as a Promise,
   * already returns an Effect.
3. Define a specific typed error with `Schema.TaggedErrorClass`.
4. Wrap non-Effect synchronous APIs with `Effect.try`.
5. Wrap Promise APIs with `Effect.tryPromise`.
6. Remove raw `try/catch` from business workflows.
7. Compose Effects with `yield*`.
8. Handle errors only with `Effect.catchTag` / `Effect.catchTags` where recovery or translation is meaningful.
9. Do not catch-all and return vague fallback values.
10. Do not swallow defects.
11. Update tests for both success and failure paths.

---

## When Raw `try/catch` May Remain

Raw `try/catch` is acceptable only in limited cases.

### 1. Legacy boundary code not yet migrated to Effect

If an older module still uses `async` / `await`, a temporary `try/catch` may remain. Prefer migrating the inside of the module to Effect first.

### 2. Framework-enforced synchronous boundaries

Some framework callbacks cannot directly return Effect. In that case, run the Effect at the outermost possible layer and handle errors there.

### 3. Temporary third-party compatibility

If a third-party library has unstable or poorly typed behavior, a local `try/catch` may exist in an adapter layer.

Do not spread this pattern into domain or application services.

### 4. Test framework limitations

Some tests may need to catch runtime behavior, but prefer asserting on Effect results, typed errors, or Exit values.

---

## Forbidden Patterns

### 1. Swallowing errors

Forbidden:

```ts
try {
  await syncUser()
} catch {
  return
}
```

Allowed only when the business case explicitly says the error is safe to ignore.

Preferred:

```ts
const program = syncUser.pipe(
  Effect.catchTag("UserAlreadySynced", () => Effect.void)
)
```

---

### 2. Returning fake fallback data

Forbidden:

```ts
try {
  return await getUser(id)
} catch {
  return {
    id,
    name: "Unknown"
  }
}
```

Preferred:

```ts
const program = getUser(id).pipe(
  Effect.catchTag("UserNotFound", () =>
    Effect.succeed(Option.none<User>())
  )
)
```

---

### 3. Catch-all conversion to generic `Error`

Forbidden:

```ts
try {
  return await doWork()
} catch {
  throw new Error("Something went wrong")
}
```

Preferred:

```ts
export class RemoteServiceError extends Schema.TaggedErrorClass<RemoteServiceError>()(
  "RemoteServiceError",
  {
    service: Schema.String,
    cause: Schema.Defect
  }
) {}
```

---

### 4. Re-handling the same error at every layer

Avoid wrapping the same error repeatedly in repository, service, controller, and route layers.

Preferred separation:

* Repository layer translates database errors.
* Domain layer models business rules.
* Application layer composes workflows.
* Boundary layer maps typed errors to external responses.

---

## Recommended Project Structure

```txt
src/
  domain/
    user/
      UserErrors.ts
      UserService.ts
  infra/
    db/
      Database.ts
      DatabaseErrors.ts
  adapters/
    http/
      UserRoutes.ts
  shared/
    errors/
      HttpErrorMapping.ts
```

Guidelines:

* Put business errors in `domain/**/Errors.ts`.
* Put infrastructure errors in `infra/**/Errors.ts`.
* Put framework and protocol translation in `adapters/**`.
* Do not put HTTP status codes in domain errors.
* Do not leak third-party SDK errors into the domain layer.

---

## Example Migration

### Before

```ts
export async function parseAndSaveUser(raw: string) {
  try {
    const input = JSON.parse(raw)
    const user = await db.user.create({ data: input })
    return user
  } catch (error) {
    console.error("Failed to save user", error)
    throw new Error("Failed to save user")
  }
}
```

Problems:

* JSON parsing errors and database errors are mixed together.
* Error type information is lost.
* Logging is coupled to business logic.
* Callers can only see a generic `Error`.
* Precise recovery is impossible.

---

### After

```ts
import { Effect, Schema } from "effect"

export class JsonParseError extends Schema.TaggedErrorClass<JsonParseError>()("JsonParseError", {
  input: Schema.String,
  cause: Schema.Defect
}) {}

export class UserCreateError extends Schema.TaggedErrorClass<UserCreateError>()("UserCreateError", {
  cause: Schema.Defect
}) {}

export const parseUserInput = Effect.fn("parseUserInput")((raw: string) =>
  Effect.try({
    try: () => JSON.parse(raw) as CreateUserInput,
    catch: (cause) => new JsonParseError({ input: raw, cause })
  })
)

export const createUserRecord = Effect.fn("createUserRecord")((input: CreateUserInput) =>
  Effect.tryPromise({
    try: () => db.user.create({ data: input }),
    catch: (cause) => new UserCreateError({ cause })
  })
)

export const parseAndSaveUser = Effect.fn("parseAndSaveUser")(function* (raw: string) {
  const input = yield* parseUserInput(raw)
  const user = yield* createUserRecord(input)
  return user
})
```

HTTP boundary:

```ts
export const handleCreateUser = (raw: string) =>
  parseAndSaveUser(raw).pipe(
    Effect.catchTag("JsonParseError", () =>
      Effect.succeed({
        status: 400 as const,
        body: {
          message: "Invalid JSON"
        }
      })
    ),
    Effect.catchTag("UserCreateError", () =>
      Effect.succeed({
        status: 500 as const,
        body: {
          message: "Failed to create user"
        }
      })
    )
  )
```

---

## Connection to The Pragmatic Programmer

### DRY

Do not duplicate knowledge about:

* How errors are detected
* How errors are converted
* How errors are logged
* How fallback values are chosen
* How HTTP responses are produced

Centralize that knowledge:

* Adapter layer converts third-party failures.
* Domain layer defines business errors.
* Boundary layer maps errors to external responses.
* Observability layer handles logging and tracing.

---

### Dead Programs Tell No Lies

Do not hide impossible states.

If an invariant is broken, fail clearly instead of continuing with corrupted data.

Bad:

```ts
try {
  return calculateBalance(account)
} catch {
  return 0
}
```

Better:

```ts
if (account.currency !== expectedCurrency) {
  return yield* Effect.dieMessage("Invariant violated: unexpected account currency")
}
```

---

### Finish What You Start

The code that acquires a resource should define how it is released.

Bad:

```ts
const lock = await acquireLock()

try {
  await runJob()
} finally {
  await lock.release()
}
```

Better:

```ts
const acquireJobLock = Effect.acquireRelease(
  Effect.tryPromise({
    try: () => acquireLock(),
    catch: (cause) => new LockAcquireError({ cause })
  }),
  (lock) => Effect.promise(() => lock.release())
)
```

---

### Take Small Steps

Do not rewrite the entire project at once.

Migrate in this order:

1. Repeated `try/catch` blocks
2. Third-party API adapters
3. Database, HTTP, and filesystem boundaries
4. Core business workflows
5. Resource management code
6. HTTP / CLI / Worker entrypoints

After each migration, update or add tests.

---

## Code Review Checklist

Before submitting changes, verify:

* [ ] New business functions return Effect.
* [ ] Raw `try/catch` is avoided in business logic.
* [ ] Synchronous throwing APIs are wrapped with `Effect.try`.
* [ ] Promise APIs are wrapped with `Effect.tryPromise`.
* [ ] Errors are specific and typed.
* [ ] There is no catch-all conversion to vague errors.
* [ ] Errors are handled only where recovery or translation is meaningful.
* [ ] Defects are not swallowed.
* [ ] Domain code does not contain HTTP status codes.
* [ ] Resource management uses Effect constructs instead of manual `try/finally`.
* [ ] The code avoids catch-log-rethrow.
* [ ] Tests cover both success and error paths.

---

## Default Rule

When unsure, use this default:

> In business code, remove raw `try/catch` and represent failures with typed Effect errors. At non-Effect boundaries, use `Effect.try` or `Effect.tryPromise`. At HTTP, CLI, Worker, or framework boundaries, run the Effect and translate errors into external responses.

Any exception should be local, intentional, and documented near the code.
