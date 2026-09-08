# Share one Library Image Cache owner across image consumers

_Status: Accepted and implemented, 2026-09-08. Refines the retained disk-cache semantics of [ADR 0017](0017-origin-encoded-library-image-cache.md) and the independent demand lifecycles of [ADR 0036](0036-demand-owned-library-image-lifecycle.md)._

Before this change, the application constructed separate Library Image and saved-profile avatar adapters whose default disk caches used the same directory but created independent locks and clear epochs. The disk-cache module could not coordinate that shared resource with duplicated ownership. Avatar authentication and demand lifetimes remain intentionally independent; merging the entire adapters would solve the wrong problem.

Construct one `ArtworkDiskCache` owner in application composition and share it through the existing construction seam with both adapters. Keep disk locking, clear epochs, origin-byte storage, eviction, and the shared byte budget behind that module's interface. Keep each adapter's authentication, demand registry, encoded-memory cache, and Library Image Rasters independent. This gives disk operations locality and both consumers leverage without adding a trait, singleton, scheduler, or second raster cache.

## Clear semantics

Clear removes existing disk entries and invalidates disk writes already started by either consumer. A network or decode operation that finishes later may start a new disk write and repopulate the cache; Clear does not cancel image demand, discard displayed images, or hold the directory empty while browsing continues. Do not propagate a clear epoch through every image load to provide a stronger promise. Coordination is within one running application; cross-process locking is not introduced.

## Verification and scope

Exercise the existing cache interface with two consumers sharing one temporary root: concurrent stores and eviction, reads during mutation, Clear during an admitted write, and writes admitted after Clear. Preserve origin-byte and disabled-cache behavior. Replace assertions about private epoch fields with coordinated observable scenarios where practical; do not add constructor-field tests.

This decision changes disk ownership, not the demand lifecycle accepted in ADR 0036. It does not resurrect the historical localhost proxy, SQLite catalog, transformed disk images, or a generic filesystem adapter. Application composition now shares the existing disk-cache owner; coordinated cache tests and an executable two-adapter smoke scenario exercise the contract.
