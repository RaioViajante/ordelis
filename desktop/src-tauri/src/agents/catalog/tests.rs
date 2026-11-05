// Provider catalog tests. Pure: nothing here locates or runs an executable. The only file read is the inherited
// runtime's source text, to compare provider ids.
use super::*;

#[test]
fn every_provider_has_its_stable_id_and_identity() {
    let ids: Vec<&str> = ProviderId::ALL.iter().map(|p| p.id()).collect();
    assert_eq!(ids, ["codex", "claude", "opencode", "kimi"]);
    for id in ProviderId::ALL {
        assert_eq!(ProviderId::from_id(id.id()), Some(id));
        assert_eq!(provider(id).id, id);
    }
    let names: Vec<(&str, &str)> = ProviderId::ALL.iter().map(|&p| (provider(p).display_name, provider(p).executable_name)).collect();
    assert_eq!(names, [("Codex", "codex"), ("Claude Code", "claude"), ("OpenCode", "opencode"), ("Kimi Code CLI", "kimi")]);
}

#[test]
fn unknown_or_near_miss_ids_are_not_providers() {
    for id in ["", "Codex", "CLAUDE", "claude-code", "claude_code", "open-code", "kimi-code", " kimi", "kimi\n", "gemini", "qwen"] {
        assert_eq!(ProviderId::from_id(id), None, "{id:?}");
    }
}

#[test]
fn inherited_adapter_ids_match_and_kimi_has_no_adapter() {
    let contracts = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/contracts.py")).unwrap();
    let line = contracts.lines().find(|l| l.starts_with("PROVIDER_IDS")).expect("PROVIDER_IDS in runtime/contracts.py");
    for id in ["codex", "claude", "opencode"] {
        assert!(line.contains(&format!("\"{id}\"")), "{id} is an inherited adapter id");
    }
    assert!(!line.contains("\"kimi\""), "kimi has no inherited adapter (#108)");
}

#[test]
fn executable_names_are_bare_names_never_paths_or_arguments() {
    for id in ProviderId::ALL {
        let name = provider(id).executable_name;
        assert!(!name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase()), "{name:?}");
    }
}

#[test]
fn every_provider_states_every_feature_exactly_once_in_order() {
    for id in ProviderId::ALL {
        let features: Vec<Feature> = provider(id).claims.iter().map(|c| c.feature).collect();
        assert_eq!(features, Feature::ALL, "{}", id.id());
        for feature in Feature::ALL {
            assert_eq!(provider(id).claim(feature).map(|c| c.feature), Some(feature));
        }
    }
}

#[test]
fn every_statement_names_its_source_date_and_how_it_was_read() {
    for id in ProviderId::ALL {
        let entry = provider(id);
        for claim in entry.claims {
            let label = format!("{} {:?}", id.id(), claim.feature);
            // Notes are short, single-spaced prose (a line continuation that kept indentation would show as "  ").
            assert!(!claim.note.is_empty() && claim.note.len() <= 300, "{label}");
            assert!(!claim.note.contains("  ") && !claim.note.contains('\n'), "{label}");
            match claim.status {
                // Unreviewed means unread and unsourced, and nothing else may be.
                DocStatus::NotReviewed => {
                    assert_eq!((claim.reading, claim.reference), (Reading::NotRead, None), "{label}");
                }
                _ => {
                    assert_ne!(claim.reading, Reading::NotRead, "{label}");
                    assert!(claim.reference.is_some(), "{label}");
                }
            }
        }
        let references = entry.claims.iter().filter_map(|c| c.reference).chain([entry.executable_name_reference]);
        for reference in references {
            assert!(reference.url.starts_with("https://") && !reference.url.contains(char::is_whitespace), "{}", reference.url);
            assert!(!reference.title.is_empty());
            assert_eq!(reference.retrieved_on, Date { year: 2026, month: 11, day: 5 });
            assert!(reference.page_updated.is_none_or(|updated| updated <= reference.retrieved_on));
        }
    }
}

// The statements below are the ones the D3.2 design relies on. Changing one is a review decision, not an edit.
#[test]
fn the_reviewed_uncertainties_and_absences_stay_explicit() {
    let status = |id, feature| provider(id).claim(feature).map(|c| c.status);
    // Kimi's documentation contradicts itself on the permission default of non-interactive mode.
    assert_eq!(status(ProviderId::Kimi, Feature::PermissionModes), Some(DocStatus::Contradictory));
    // The inherited runtime's ACP launch commands for Codex and Claude Code are not documented by either vendor.
    assert_eq!(status(ProviderId::Codex, Feature::AgentClientProtocol), Some(DocStatus::NotFound));
    assert_eq!(status(ProviderId::Claude, Feature::AgentClientProtocol), Some(DocStatus::NotFound));
    // Only Claude Code documents a caller-chosen session id.
    for id in ProviderId::ALL {
        let expected = if id == ProviderId::Claude { DocStatus::Documented } else if id == ProviderId::Kimi { DocStatus::NotReviewed } else { DocStatus::NotFound };
        assert_eq!(status(id, Feature::HostAssignedSessionId), Some(expected), "{}", id.id());
    }
    // Codex documents no approval routing; its app-server is Experimental.
    assert_eq!(status(ProviderId::Codex, Feature::ApprovalRouting), Some(DocStatus::NotFound));
    // Kimi was read from excerpts only, and the parts nobody read say so.
    let kimi = provider(ProviderId::Kimi);
    assert!(kimi.claims.iter().all(|c| c.reading != Reading::Verbatim));
    assert!(kimi.claims.iter().any(|c| c.status == DocStatus::NotReviewed));
    assert!(kimi.notes.iter().any(|n| n.contains("decision gate is open")));
}

#[test]
fn provider_sandboxes_are_described_as_the_providers_own() {
    // A documented provider sandbox is the provider's claim about itself. The evidence vocabulary keeps it at the
    // weakest level; the catalog wording must not suggest confinement by Ordelis.
    for id in ProviderId::ALL {
        let note = provider(id).claim(Feature::ProviderSandbox).unwrap().note.to_lowercase();
        assert!(!note.contains("ordelis") && !note.contains("secure") && !note.contains("guarantee"), "{note}");
    }
    let claude = provider(ProviderId::Claude).claim(Feature::ProviderSandbox).unwrap().note;
    assert!(claude.contains("shell commands only") && claude.contains("file tools"), "{claude}");
}

#[test]
fn the_catalog_holds_no_local_path_or_command_line() {
    let mut texts: Vec<&str> = Vec::new();
    for id in ProviderId::ALL {
        let entry = provider(id);
        texts.extend([entry.display_name, entry.executable_name]);
        texts.extend(entry.notes.iter().copied());
        texts.extend(entry.claims.iter().map(|c| c.note));
    }
    for text in texts {
        for marker in ["/Users/", "/opt/", "/usr/", "/bin/", "~/", "$HOME", "%APPDATA%", "C:\\", ".exe"] {
            assert!(!text.contains(marker), "{marker} in {text:?}");
        }
    }
}
