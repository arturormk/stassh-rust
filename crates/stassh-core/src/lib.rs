pub mod action;
pub mod export;
pub mod frontend;
pub mod identity;
pub mod import;
pub mod local;
pub mod model;
pub mod openssh;
pub mod secrets;
pub mod simulation;
pub mod storage;

pub use action::{
    ActionError, ResolvedActionPlan, ResolvedLocalCommand, parse_prepare_env,
    resolve_action_local_prepare, resolve_action_plan,
};
pub use export::export_openssh_config;
pub use frontend::{
    ensure_home_stassh_permissions, local_config_path, prepare_openssh_command, secrets_path,
    selector, vault_path,
};
pub use identity::{
    DerivedIdentity, IdentityDeriveError, IdentityFileResolver, OpenSshIdentityResolver,
    derive_identity_from_file, parse_ssh_keygen_fingerprint_output,
};
pub use import::{
    IdentityImportContext, ImportOpenSshSummary, OpenSshConfigRead, OpenSshConfigReadError,
    import_openssh_config, import_openssh_config_with_identities,
    read_openssh_config_with_includes,
};
pub use local::{
    CapabilityMapping, IdentityMapping, LocalConfig, LocalConfigError, load_local_config,
    save_local_config,
};
pub use model::{
    ActionDefinition, ActionForwardDefinition, ActionLocalCommand, ActionPort, AddFolder, AddHost,
    DuplicateHostEntry, DuplicateHostGroup, DuplicateHostKind, Folder, ForwardDefinition, Host,
    HostDedupeGroup, HostDedupePlan, HostDedupeResult, HostDedupeStrategy, HostSelector,
    ResolvedHost, StasshError, UpdateHost, Vault,
};
pub use openssh::{OpenSshCommand, OpenSshConfig, TempOpenSshConfig};
pub use secrets::{
    EncryptedSecret, SecretField, SecretPlaintext, SecretRecordKind, SecretSet, SecretsCrypto,
    SecretsError, SecretsKey, SecretsStore, load_secrets, save_secrets,
};
pub use simulation::{
    SIMULATION_MASTER_PASSWORD, SimulatedOutput, SimulatedShell, SimulationWorkspace,
    demo_workspace,
};
pub use storage::{load_vault, save_vault};
