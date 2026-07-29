use vzdc_discord_bot::models::{
    AnnouncementPayload, ControllerEventsResponse, ControllerLifecyclePayload, DiscordConfigBundle,
    Event, EventPositionListResponse, EventPositionPostingPayload,
};

fn fixture(name: &str) -> String {
    let path = format!("tests/fixtures/osmium/{name}");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
}

#[test]
fn deserialize_announcement_payload() {
    let json = fixture("announcement_payload.json");
    let payload: AnnouncementPayload = serde_json::from_str(&json).unwrap();

    assert_eq!(payload.title, "ZDC Maintenance Window");
    assert!(payload.body_markdown.contains("scheduled maintenance"));
    assert_eq!(
        payload.details_url.as_deref(),
        Some("https://vzdc.org/announcements/maintenance-window-2026-05-17")
    );
    assert_eq!(payload.requested_by_cid, 1234567);
}

#[test]
fn announcement_payload_round_trips() {
    let json = fixture("announcement_payload.json");
    let payload: AnnouncementPayload = serde_json::from_str(&json).unwrap();
    let serialized = serde_json::to_string(&payload).unwrap();
    let reparsed: AnnouncementPayload = serde_json::from_str(&serialized).unwrap();
    assert_eq!(payload, reparsed);
}

#[test]
fn deserialize_event_position_posting_payload() {
    let json = fixture("event_position_posting_payload.json");
    let payload: EventPositionPostingPayload = serde_json::from_str(&json).unwrap();

    assert_eq!(payload.event_id, "evt_2026_spring_fno");
    assert!(payload.ping_users);
    assert_eq!(payload.requested_by_cid, 1234567);
}

#[test]
fn event_position_posting_payload_round_trips() {
    let json = fixture("event_position_posting_payload.json");
    let payload: EventPositionPostingPayload = serde_json::from_str(&json).unwrap();
    let serialized = serde_json::to_string(&payload).unwrap();
    let reparsed: EventPositionPostingPayload = serde_json::from_str(&serialized).unwrap();
    assert_eq!(payload, reparsed);
}

#[test]
fn deserialize_event_response() {
    let json = fixture("event_response.json");
    let event: Event = serde_json::from_str(&json).unwrap();

    assert_eq!(event.id, "evt_2026_spring_fno");
    assert_eq!(event.title, "Spring FNO 2026");
    assert_eq!(event.event_type.as_deref(), Some("fno"));
    assert_eq!(event.host.as_deref(), Some("VATUSA"));
    assert!(
        event
            .description
            .as_ref()
            .unwrap()
            .contains("Friday Night Operations")
    );
    assert_eq!(event.status, "published");
    assert!(event.published);
    assert_eq!(event.created_by, "usr_admin_001");
}

#[test]
fn deserialize_event_positions_response() {
    let json = fixture("event_positions_response.json");
    let response: EventPositionListResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(response.total, 3);
    assert_eq!(response.items.len(), 3);
    assert!(!response.has_next);
    assert!(!response.has_prev);

    let iad = response
        .items
        .iter()
        .find(|p| p.callsign == "IAD_CTR")
        .unwrap();
    assert_eq!(iad.user_id.as_deref(), Some("usr_123"));
    assert_eq!(iad.status, "assigned");

    let dca = response
        .items
        .iter()
        .find(|p| p.callsign == "DCA_TWR")
        .unwrap();
    assert!(dca.user_id.is_none());
    assert_eq!(dca.status, "open");
}

#[test]
fn deserialize_discord_configs_response() {
    let json = fixture("discord_configs_response.json");
    let bundle: DiscordConfigBundle = serde_json::from_str(&json).unwrap();

    assert_eq!(bundle.configs.len(), 1);
    assert_eq!(bundle.configs[0].name, "main");
    assert_eq!(
        bundle.configs[0].guild_id.as_deref(),
        Some("123456789012345678")
    );

    assert_eq!(bundle.channels.len(), 6);
    let announcements = bundle
        .channels
        .iter()
        .find(|c| c.name == "announcements")
        .unwrap();
    assert_eq!(announcements.channel_id, "111111111111111111");

    assert_eq!(bundle.roles.len(), 4);
    let impromptu_s1 = bundle
        .roles
        .iter()
        .find(|r| r.name == "impromptu_s1")
        .unwrap();
    assert_eq!(impromptu_s1.role_id, "777777777777777771");

    assert_eq!(bundle.categories.len(), 1);
    assert_eq!(bundle.categories[0].name, "operations");
}

#[test]
fn discord_config_bundle_resolves_channels() {
    let json = fixture("discord_configs_response.json");
    let bundle: DiscordConfigBundle = serde_json::from_str(&json).unwrap();

    let targets = bundle.resolve_targets().unwrap();
    assert_eq!(targets.announcements.len(), 1);
    assert_eq!(targets.event_postings.len(), 1);
    assert_eq!(targets.staffup.len(), 1);
    assert_eq!(targets.audit_log.len(), 1);
}

#[test]
fn discord_config_bundle_resolves_role_prefixes() {
    let json = fixture("discord_configs_response.json");
    let bundle: DiscordConfigBundle = serde_json::from_str(&json).unwrap();

    let impromptu_roles = bundle.resolve_role_prefix("impromptu_").unwrap();
    assert_eq!(impromptu_roles.len(), 2);

    let break_board_roles = bundle.resolve_role_prefix("break_board_").unwrap();
    assert_eq!(break_board_roles.len(), 2);
}

#[test]
fn deserialize_controller_events_response() {
    let json = fixture("controller_events_response.json");
    let response: ControllerEventsResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(response.environment, "live");
    assert_eq!(response.events.len(), 4);

    let logon = &response.events[0];
    assert_eq!(logon.event_type, "controller_logged_on");
    assert_eq!(logon.cid, 1234567);
    assert_eq!(logon.session_id.as_deref(), Some("sess_abc123"));

    let activated = &response.events[1];
    assert_eq!(activated.event_type, "position_activated");
    assert_eq!(activated.activation_id.as_deref(), Some("act_xyz789"));

    let deactivated = &response.events[2];
    assert_eq!(deactivated.event_type, "position_deactivated");

    let logoff = &response.events[3];
    assert_eq!(logoff.event_type, "controller_logged_off");
}

#[test]
fn controller_lifecycle_payload_deserializes_logon() {
    let json = r#"{
        "event_type": "controller_logged_on",
        "data": {
            "environment": "live",
            "artcc_id": "ZDC",
            "cid": 1234567,
            "user_id": "usr_123",
            "session_id": "sess_abc123",
            "occurred_at": "2026-05-11T18:00:00Z",
            "real_name": "John Doe",
            "role": "controller",
            "user_rating": "S3",
            "requested_rating": "S3",
            "primary_facility_id": "IAD",
            "primary_position_id": "pos_iad_app"
        }
    }"#;

    let payload: ControllerLifecyclePayload = serde_json::from_str(json).unwrap();
    match payload {
        ControllerLifecyclePayload::ControllerLoggedOn(data) => {
            assert_eq!(data.cid, 1234567);
            assert_eq!(data.session_id, "sess_abc123");
            assert_eq!(data.real_name.as_deref(), Some("John Doe"));
        }
        _ => panic!("expected ControllerLoggedOn variant"),
    }
}

#[test]
fn controller_lifecycle_payload_deserializes_position_activated() {
    let json = r#"{
        "event_type": "position_activated",
        "data": {
            "environment": "live",
            "artcc_id": "ZDC",
            "cid": 1234567,
            "user_id": "usr_123",
            "session_id": "sess_abc123",
            "activation_id": "act_xyz789",
            "occurred_at": "2026-05-11T18:00:05Z",
            "real_name": "John Doe",
            "role": "controller",
            "user_rating": "S3",
            "requested_rating": "S3",
            "position_id": "pos_iad_app",
            "facility_id": "IAD",
            "facility_name": "Washington Dulles",
            "position_name": "Dulles Approach",
            "position_type": "approach",
            "radio_name": "Dulles Approach",
            "default_callsign": "IAD_APP",
            "frequency": 120.45,
            "is_primary": true
        }
    }"#;

    let payload: ControllerLifecyclePayload = serde_json::from_str(json).unwrap();
    match payload {
        ControllerLifecyclePayload::PositionActivated(data) => {
            assert_eq!(data.cid, 1234567);
            assert_eq!(data.activation_id, "act_xyz789");
            assert_eq!(data.default_callsign.as_deref(), Some("IAD_APP"));
            assert_eq!(data.frequency, Some(120.45));
            assert!(data.is_primary);
        }
        _ => panic!("expected PositionActivated variant"),
    }
}

#[test]
fn controller_lifecycle_payload_deserializes_logoff() {
    let json = r#"{
        "event_type": "controller_logged_off",
        "data": {
            "environment": "live",
            "artcc_id": "ZDC",
            "cid": 1234567,
            "user_id": "usr_123",
            "session_id": "sess_abc123",
            "occurred_at": "2026-05-11T19:30:05Z",
            "real_name": "John Doe",
            "role": "controller",
            "user_rating": "S3",
            "requested_rating": "S3",
            "primary_facility_id": "IAD",
            "primary_position_id": "pos_iad_app"
        }
    }"#;

    let payload: ControllerLifecyclePayload = serde_json::from_str(json).unwrap();
    match payload {
        ControllerLifecyclePayload::ControllerLoggedOff(data) => {
            assert_eq!(data.cid, 1234567);
            assert_eq!(data.session_id, "sess_abc123");
        }
        _ => panic!("expected ControllerLoggedOff variant"),
    }
}

#[test]
fn controller_lifecycle_payload_deserializes_position_deactivated() {
    let json = r#"{
        "event_type": "position_deactivated",
        "data": {
            "environment": "live",
            "artcc_id": "ZDC",
            "cid": 1234567,
            "user_id": "usr_123",
            "session_id": "sess_abc123",
            "activation_id": "act_xyz789",
            "occurred_at": "2026-05-11T19:30:00Z",
            "real_name": "John Doe",
            "role": "controller",
            "user_rating": "S3",
            "requested_rating": "S3",
            "position_id": "pos_iad_app",
            "facility_id": "IAD",
            "facility_name": "Washington Dulles",
            "position_name": "Dulles Approach",
            "position_type": "approach",
            "radio_name": "Dulles Approach",
            "default_callsign": "IAD_APP",
            "frequency": 120.45,
            "is_primary": true
        }
    }"#;

    let payload: ControllerLifecyclePayload = serde_json::from_str(json).unwrap();
    match payload {
        ControllerLifecyclePayload::PositionDeactivated(data) => {
            assert_eq!(data.cid, 1234567);
            assert_eq!(data.activation_id, "act_xyz789");
        }
        _ => panic!("expected PositionDeactivated variant"),
    }
}
