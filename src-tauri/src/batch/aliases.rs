//! Batch-local exact identities. Similar names are suggestions, never automatic merges.
use super::*;
use crate::privacy::{Entity, Kind};
use serde::Deserialize;
#[derive(Clone)]
pub(super) struct Identity {
    id: String,
    kind: Kind,
    variants: Vec<String>,
    alias: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdentityView {
    id: String,
    kind: Kind,
    variants: Vec<String>,
    alias: String,
    documents: usize,
    reviewed: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasCatalog {
    groups: Vec<IdentityView>,
    suggestions: Vec<[String; 2]>,
}
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum AliasAction {
    Map {
        entities: Vec<Entity>,
    },
    Rename {
        group_id: String,
        alias: String,
    },
    Merge {
        source_id: String,
        target_id: String,
    },
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasChange {
    documents: usize,
    reviewed: usize,
    exported: usize,
    rebuilt: usize,
    session: Option<Session>,
}
fn key_matches(g: &Identity, e: &Entity) -> bool {
    g.kind == e.kind && g.variants.contains(&e.original)
}
fn register(batch: &mut Batch, entity: &Entity, automatic: bool) -> Result<String, String> {
    if let Some(group) = batch.identities.iter().find(|g| key_matches(g, entity)) {
        return Ok(group.alias.clone());
    }
    let alias = if automatic {
        let counter = batch.next_alias.entry(entity.kind).or_default();
        loop {
            *counter += 1;
            let candidate = format!("{}_{:02}", entity.kind.prefix(), counter);
            if !batch.identities.iter().any(|g| g.alias == candidate) {
                break candidate;
            }
        }
    } else {
        if batch.identities.iter().any(|g| g.alias == entity.alias) {
            return Err("Ese alias ya identifica otra entidad del lote. Usa Unir variantes para relacionarlas.".into());
        }
        entity.alias.clone()
    };
    batch.identities.push(Identity {
        id: uuid::Uuid::new_v4().to_string(),
        kind: entity.kind,
        variants: vec![entity.original.clone()],
        alias: alias.clone(),
    });
    Ok(alias)
}
pub(super) fn reconcile(batch: &mut Batch, session: &Session) -> Result<Session, String> {
    if session.entities.is_empty() {
        return Ok(session.clone());
    }
    let mut entities = session.entities.clone();
    for e in &mut entities {
        e.alias = register(batch, e, true)?;
    }
    crate::privacy::batch_map(session, entities)
}
fn references(session: &Session, identity: &Identity) -> bool {
    session.entities.iter().any(|e| key_matches(identity, e))
}
fn tokens(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_alphabetic())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .filter(|t| {
            !matches!(
                t.as_str(),
                "sr" | "sra"
                    | "señor"
                    | "señora"
                    | "dr"
                    | "dra"
                    | "don"
                    | "doña"
                    | "mr"
                    | "mrs"
                    | "ms"
            )
        })
        .collect()
}
fn possible_variant(a: &[String], b: &[String]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.last() != b.last() || a.last().is_none_or(|s| s.chars().count() < 3) {
        return false;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.iter().all(|s| {
        long.iter()
            .any(|l| s == l || (s.chars().count() == 1 && s.chars().next() == l.chars().next()))
    })
}
fn catalog(batch: &Batch) -> AliasCatalog {
    let mut groups = Vec::new();
    for identity in &batch.identities {
        let docs = batch
            .items
            .iter()
            .filter_map(|i| i.session.as_ref())
            .filter(|s| references(s, identity))
            .collect::<Vec<_>>();
        if docs.is_empty() {
            continue;
        }
        groups.push(IdentityView {
            id: identity.id.clone(),
            kind: identity.kind,
            variants: identity.variants.clone(),
            alias: identity.alias.clone(),
            documents: docs.len(),
            reviewed: docs.iter().filter(|s| s.reviewed).count(),
        });
    }
    let people = groups
        .iter()
        .filter(|g| g.kind == Kind::Person)
        .map(|g| (g, g.variants.iter().map(|v| tokens(v)).collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    let mut suggestions = Vec::new();
    let mut comparisons = 0;
    'outer: for (i, (a, aa)) in people.iter().enumerate() {
        for (b, bb) in people.iter().skip(i + 1) {
            comparisons += 1;
            if comparisons > 50_000 || suggestions.len() >= 30 {
                break 'outer;
            }
            if aa.iter().any(|x| bb.iter().any(|y| possible_variant(x, y))) {
                suggestions.push([a.id.clone(), b.id.clone()]);
            }
        }
    }
    AliasCatalog {
        groups,
        suggestions,
    }
}
#[tauri::command]
pub(crate) fn get_private_batch_aliases(app: tauri::AppHandle) -> Result<AliasCatalog, String> {
    let state = app.state::<BatchState>();
    let mut batch = state.0.lock().map_err(|_| locked())?;
    if batch.processing {
        return Err("Espera a que termine la detección del lote.".into());
    }
    sync_current(&app, &mut batch)?;
    Ok(catalog(&batch))
}
fn stage(batch: &Batch, action: AliasAction) -> Result<(Batch, AliasChange), String> {
    let mut next = batch.clone();
    let active = next
        .items
        .iter()
        .position(|i| Some(&i.id) == next.active.as_ref())
        .ok_or_else(locked)?;
    let current = next.items[active].session.clone().ok_or_else(locked)?;
    let mut active_entities = None;
    match action {
        AliasAction::Map { mut entities } => {
            let mut changes = std::collections::HashMap::new();
            for e in &entities {
                if current
                    .entities
                    .iter()
                    .any(|old| old.kind == e.kind && old.original == e.original)
                {
                    if let Some(group) = next.identities.iter().find(|g| key_matches(g, e)) {
                        if let Some(known) = changes.insert(group.id.clone(), e.alias.clone()) {
                            if known != e.alias {
                                return Err("Un alias compartido debe mantener el mismo reemplazo en el texto editado.".into());
                            }
                        }
                    }
                }
            }
            for group in &mut next.identities {
                if let Some(alias) = changes.get(&group.id) {
                    group.alias = alias.clone();
                }
            }
            for e in &mut entities {
                e.alias = register(&mut next, e, false)?;
            }
            active_entities = Some(entities);
        }
        AliasAction::Rename { group_id, alias } => {
            let group = next
                .identities
                .iter_mut()
                .find(|g| g.id == group_id)
                .ok_or_else(locked)?;
            group.alias = alias;
        }
        AliasAction::Merge {
            source_id,
            target_id,
        } => {
            if source_id == target_id {
                return Err("Selecciona dos identidades distintas.".into());
            }
            let source = next
                .identities
                .iter()
                .find(|g| g.id == source_id)
                .cloned()
                .ok_or_else(locked)?;
            let target = next
                .identities
                .iter_mut()
                .find(|g| g.id == target_id)
                .ok_or_else(locked)?;
            if source.kind != target.kind {
                return Err("Solo se pueden unir variantes de la misma categoría.".into());
            }
            for variant in source.variants {
                if !target.variants.contains(&variant) {
                    target.variants.push(variant);
                }
            }
            next.identities.retain(|g| g.id != source_id);
        }
    }
    let private_values = next
        .identities
        .iter()
        .flat_map(|g| g.variants.clone())
        .collect::<Vec<_>>();
    let mut seen = std::collections::HashSet::new();
    for group in &next.identities {
        if crate::privacy::batch_alias_leaks(&group.alias, &private_values) {
            return Err(
                "El alias coincide con información privada o con texto original. Elige otro."
                    .into(),
            );
        }
        if group.alias.trim().is_empty()
            || group.alias.len() > 120
            || group.alias.contains(['\n', '\r'])
        {
            return Err("Escribe un alias de 1 a 120 caracteres, sin saltos de línea.".into());
        }
        if !seen.insert(group.alias.as_str()) {
            return Err("Ese alias ya identifica otra entidad del lote. Usa Unir variantes para relacionarlas.".into());
        }
    }
    let mut impact = AliasChange {
        documents: 0,
        reviewed: 0,
        exported: 0,
        rebuilt: 0,
        session: None,
    };
    for (index, item) in next.items.iter_mut().enumerate() {
        let Some(session) = item.session.as_ref() else {
            continue;
        };
        let mut entities = if index == active {
            active_entities
                .take()
                .unwrap_or_else(|| session.entities.clone())
        } else {
            session.entities.clone()
        };
        for entity in &mut entities {
            if let Some(group) = next.identities.iter().find(|g| key_matches(g, entity)) {
                entity.alias = group.alias.clone();
            }
        }
        let updated = crate::privacy::batch_map(session, entities)?;
        if updated.revision != session.revision {
            impact.documents += 1;
            if item.exported_revision == Some(session.revision) {
                impact.exported += 1;
            }
            if session.edited && !updated.edited {
                impact.rebuilt += 1;
            }
            if session.reviewed {
                impact.reviewed += 1;
            }
            item.exported_revision = None;
            item.error.clear();
            item.session = Some(updated);
        }
    }
    impact.session = next.items[active].session.clone();
    Ok((next, impact))
}
#[tauri::command]
pub(crate) fn change_private_batch_aliases(
    app: tauri::AppHandle,
    batch_id: String,
    id: String,
    revision: u64,
    action: AliasAction,
    commit: bool,
) -> Result<AliasChange, String> {
    let state = app.state::<BatchState>();
    let mut batch = state.0.lock().map_err(|_| locked())?;
    if batch.id != batch_id || !batch.started || batch.processing {
        return Err(locked());
    }
    sync_current(&app, &mut batch)?;
    let active = batch
        .items
        .iter()
        .find(|i| Some(&i.id) == batch.active.as_ref())
        .and_then(|i| i.session.as_ref())
        .ok_or_else(locked)?;
    if active.id != id || active.revision != revision {
        return Err("El documento cambió. Recarga la sesión.".into());
    }
    let (next, mut impact) = stage(&batch, action)?;
    if commit {
        let privacy = app.state::<PrivacyState>();
        let mut current = privacy.0.lock().map_err(|_| locked())?;
        if !current
            .as_ref()
            .is_some_and(|s| s.id == id && s.revision == revision)
        {
            return Err("El documento cambió. Recarga la sesión.".into());
        }
        *current = impact.session.clone();
        *batch = next;
    } else {
        impact.session = None;
    }
    Ok(impact)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entity(name: &str, alias: &str) -> Entity {
        Entity {
            kind: Kind::Person,
            original: name.into(),
            alias: alias.into(),
            source: "review".into(),
        }
    }
    fn add(batch: &mut Batch, name: &str) -> String {
        let mut session = Session::new("file.txt".into(), format!("{name} firma.")).unwrap();
        session.analyzed = true;
        session.analysis_complete = true;
        session.entities = vec![entity(name, "PERSON_01")];
        session.sanitized = "PERSON_01 firma.".to_string();
        let session = reconcile(batch, &session).unwrap();
        let id = session.id.clone();
        batch.items.push(Item {
            id: id.clone(),
            name: "file.txt".into(),
            source: String::new(),
            status: "review".into(),
            error: String::new(),
            session: Some(session),
            exported_revision: None,
        });
        batch.active = Some(id.clone());
        id
    }
    #[test]
    fn exact_names_share_aliases_and_different_people_never_reuse_numbers() {
        let mut b = Batch::default();
        add(&mut b, "Juan Pérez");
        add(&mut b, "Ana Ruiz");
        add(&mut b, "Juan Pérez");
        assert_eq!(
            b.items[0].session.as_ref().unwrap().entities[0].alias,
            b.items[2].session.as_ref().unwrap().entities[0].alias
        );
        assert_ne!(
            b.items[0].session.as_ref().unwrap().entities[0].alias,
            b.items[1].session.as_ref().unwrap().entities[0].alias
        );
    }
    #[test]
    fn shared_rename_preserves_text_edits_and_invalidates_only_affected_reviews() {
        let mut b = Batch::default();
        add(&mut b, "Juan Pérez");
        add(&mut b, "Ana Ruiz");
        add(&mut b, "Juan Pérez");
        for i in &mut b.items {
            i.session.as_mut().unwrap().reviewed = true;
        }
        let first = b.items[0].session.as_mut().unwrap();
        first.edited = true;
        first.sanitized.push_str(" Corregido.");
        let g = b
            .identities
            .iter()
            .find(|g| g.variants.contains(&"Juan Pérez".into()))
            .unwrap()
            .id
            .clone();
        let (next, impact) = stage(
            &b,
            AliasAction::Rename {
                group_id: g,
                alias: "CLIENTE_01".into(),
            },
        )
        .unwrap();
        assert_eq!(impact.documents, 2);
        assert_eq!(impact.reviewed, 2);
        assert!(!next.items[0].session.as_ref().unwrap().reviewed);
        assert!(next.items[1].session.as_ref().unwrap().reviewed);
        assert!(next.items[0]
            .session
            .as_ref()
            .unwrap()
            .sanitized
            .contains("Corregido."));
        assert!(next.items[0]
            .session
            .as_ref()
            .unwrap()
            .sanitized
            .starts_with("CLIENTE_01"));
        assert!(
            b.items[0].session.as_ref().unwrap().reviewed,
            "preview does not mutate originals"
        );
    }
    #[test]
    fn variants_are_only_suggestions_until_explicit_merge() {
        let mut b = Batch::default();
        add(&mut b, "Juan Pérez");
        add(&mut b, "Sr. Pérez");
        add(&mut b, "José Pérez");
        let c = catalog(&b);
        assert_eq!(c.groups.len(), 3);
        assert!(!c.suggestions.is_empty());
        let source = b.identities[1].id.clone();
        let target = b.identities[0].id.clone();
        let (next, impact) = stage(
            &b,
            AliasAction::Merge {
                source_id: source,
                target_id: target,
            },
        )
        .unwrap();
        assert_eq!(impact.documents, 1);
        assert_eq!(catalog(&next).groups.len(), 2);
        assert_eq!(
            next.items[0].session.as_ref().unwrap().entities[0].alias,
            next.items[1].session.as_ref().unwrap().entities[0].alias
        );
        assert_ne!(
            next.items[0].session.as_ref().unwrap().entities[0].alias,
            next.items[2].session.as_ref().unwrap().entities[0].alias
        );
    }
    #[test]
    fn collisions_fail_atomically() {
        let mut b = Batch::default();
        add(&mut b, "Juan Pérez");
        add(&mut b, "Ana Ruiz");
        let result = stage(
            &b,
            AliasAction::Rename {
                group_id: b.identities[0].id.clone(),
                alias: b.identities[1].alias.clone(),
            },
        );
        assert!(result.is_err());
        assert_eq!(
            b.items[0].session.as_ref().unwrap().entities[0].alias,
            "PERSON_01"
        );
    }
    #[test]
    fn saving_an_alias_in_one_private_map_updates_exact_matches_in_other_documents() {
        let mut b = Batch::default();
        add(&mut b, "Juan Pérez");
        add(&mut b, "Ana Ruiz");
        add(&mut b, "Juan Pérez");
        let mut entities = b.items[2].session.as_ref().unwrap().entities.clone();
        entities[0].alias = "CLIENTE_01".into();
        let (next, impact) = stage(&b, AliasAction::Map { entities }).unwrap();
        assert_eq!(impact.documents, 2);
        assert_eq!(
            next.items[0].session.as_ref().unwrap().entities[0].alias,
            "CLIENTE_01"
        );
        assert_eq!(
            next.items[2].session.as_ref().unwrap().entities[0].alias,
            "CLIENTE_01"
        );
        assert_eq!(
            next.items[1].session.as_ref().unwrap().entities[0].alias,
            "PERSON_02"
        );
        assert_eq!(
            b.items[0].session.as_ref().unwrap().entities[0].alias,
            "PERSON_01"
        );
    }
    #[test]
    fn shared_alias_cannot_leak_a_private_value_from_an_unaffected_document() {
        let mut b = Batch::default();
        add(&mut b, "Juan Pérez");
        add(&mut b, "Ana Ruiz");
        assert!(stage(
            &b,
            AliasAction::Rename {
                group_id: b.identities[0].id.clone(),
                alias: "Ana Ruiz".into()
            }
        )
        .is_err());
        assert_eq!(
            b.items[0].session.as_ref().unwrap().entities[0].alias,
            "PERSON_01"
        );
    }
    #[test]
    fn case_variants_are_not_automatically_assumed_to_be_the_same_person() {
        let mut b = Batch::default();
        add(&mut b, "Juan Pérez");
        add(&mut b, "JUAN PÉREZ");
        assert_eq!(b.identities.len(), 2);
    }
}
