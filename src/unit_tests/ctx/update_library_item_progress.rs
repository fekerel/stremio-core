use crate::constants::LIBRARY_RECENT_STORAGE_KEY;
use crate::models::ctx::Ctx;
use crate::runtime::msg::{Action, ActionCtx};
use crate::runtime::{Runtime, RuntimeAction};
use crate::types::events::DismissedEventsBucket;
use crate::types::library::{LibraryBucket, LibraryItem, LibraryItemState};
use crate::types::notifications::NotificationsBucket;
use crate::types::profile::Profile;
use crate::types::resource::{MetaItemBehaviorHints, MetaItemPreview, PosterShape};
use crate::types::search_history::SearchHistoryBucket;
use crate::types::server_urls::ServerUrlsBucket;
use crate::types::streams::StreamsBucket;
use crate::unit_tests::{TestEnv, NOW, STORAGE};
use chrono::{TimeZone, Utc};
use stremio_derive::Model;
use url::Url;

fn meta_preview() -> MetaItemPreview {
    MetaItemPreview {
        id: "tt123".into(),
        r#type: "series".to_owned(),
        name: "Test Series".to_owned(),
        poster: Some(Url::parse("https://example.com/poster.jpg").unwrap()),
        background: None,
        logo: None,
        description: None,
        release_info: None,
        runtime: None,
        released: None,
        poster_shape: PosterShape::Poster,
        links: vec![],
        trailer_streams: vec![],
        behavior_hints: MetaItemBehaviorHints {
            default_video_id: Some("tt123:1:1".to_owned()),
            ..Default::default()
        },
    }
}

fn test_runtime(library: LibraryBucket) -> (Runtime<TestEnv, TestModel>, impl std::any::Any) {
    Runtime::<TestEnv, _>::new(
        TestModel {
            ctx: Ctx::new(
                Profile::default(),
                library,
                StreamsBucket::default(),
                ServerUrlsBucket::new::<TestEnv>(None),
                NotificationsBucket::new::<TestEnv>(None, vec![]),
                SearchHistoryBucket::default(),
                DismissedEventsBucket::default(),
            ),
        },
        vec![],
        1000,
    )
}

#[derive(Model, Clone, Default)]
#[model(TestEnv)]
struct TestModel {
    ctx: Ctx,
}

#[test]
fn update_library_item_progress_missing_item_creates_temp_resume_item() {
    let _env_mutex = TestEnv::reset().expect("Should have exclusive lock to TestEnv");
    *NOW.write().unwrap() = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let (runtime, _rx) = test_runtime(LibraryBucket::default());

    TestEnv::run(|| {
        runtime.dispatch(RuntimeAction {
            field: None,
            action: Action::Ctx(ActionCtx::UpdateLibraryItemProgress {
                meta_item: meta_preview(),
                video_id: "tt123:1:2".to_owned(),
                time_offset: 1_000,
                duration: Some(3_600_000),
            }),
        })
    });

    let model = runtime.model().unwrap();
    let item = model.ctx.library.items.get("tt123").expect("Item exists");
    assert!(item.temp, "Missing item is created as a temporary item");
    assert!(
        item.removed,
        "Missing item preserves temp-library removed semantics"
    );
    assert_eq!(item.state.video_id.as_deref(), Some("tt123:1:2"));
    assert_eq!(item.state.time_offset, 1_000);
    assert_eq!(item.state.duration, 3_600_000);
    assert!(item.is_in_continue_watching());
    assert!(
        STORAGE
            .read()
            .unwrap()
            .get(LIBRARY_RECENT_STORAGE_KEY)
            .is_some(),
        "Library progress is persisted to storage"
    );
}

#[test]
fn update_library_item_progress_existing_item_updates_state_and_preserves_membership() {
    let _env_mutex = TestEnv::reset().expect("Should have exclusive lock to TestEnv");
    *NOW.write().unwrap() = Utc.with_ymd_and_hms(2020, 1, 2, 0, 0, 0).unwrap();
    let existing = LibraryItem {
        id: "tt123".into(),
        removed: false,
        temp: false,
        ctime: Some(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()),
        mtime: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        state: LibraryItemState {
            video_id: Some("tt123:1:1".to_owned()),
            time_offset: 500,
            time_watched: 250,
            overall_time_watched: 250,
            duration: 3_000_000,
            ..Default::default()
        },
        name: "Old Name".to_owned(),
        r#type: "series".to_owned(),
        poster: None,
        poster_shape: PosterShape::Square,
        behavior_hints: Default::default(),
    };
    let (runtime, _rx) = test_runtime(LibraryBucket {
        uid: None,
        items: vec![("tt123".into(), existing)].into_iter().collect(),
    });

    TestEnv::run(|| {
        runtime.dispatch(RuntimeAction {
            field: None,
            action: Action::Ctx(ActionCtx::UpdateLibraryItemProgress {
                meta_item: meta_preview(),
                video_id: "tt123:1:2".to_owned(),
                time_offset: 1_500,
                duration: None,
            }),
        })
    });

    let model = runtime.model().unwrap();
    let item = model.ctx.library.items.get("tt123").expect("Item exists");
    assert!(!item.temp, "Existing item keeps temp flag");
    assert!(!item.removed, "Existing item keeps removed flag");
    assert_eq!(item.name, "Test Series", "Preview metadata is refreshed");
    assert_eq!(item.poster, meta_preview().poster);
    assert_eq!(item.state.video_id.as_deref(), Some("tt123:1:2"));
    assert_eq!(item.state.time_offset, 1_500);
    assert_eq!(
        item.state.duration, 3_000_000,
        "Omitted duration preserves the existing duration"
    );
    assert_eq!(
        item.state.time_watched, 0,
        "Switching videos resets per-video watched time"
    );
    assert!(item.is_in_continue_watching());
}

#[test]
fn update_library_item_progress_zero_offset_is_noop() {
    let _env_mutex = TestEnv::reset().expect("Should have exclusive lock to TestEnv");
    *NOW.write().unwrap() = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let (runtime, _rx) = test_runtime(LibraryBucket::default());

    TestEnv::run(|| {
        runtime.dispatch(RuntimeAction {
            field: None,
            action: Action::Ctx(ActionCtx::UpdateLibraryItemProgress {
                meta_item: meta_preview(),
                video_id: "tt123:1:2".to_owned(),
                time_offset: 0,
                duration: Some(3_600_000),
            }),
        })
    });

    assert!(
        runtime.model().unwrap().ctx.library.items.is_empty(),
        "Zero progress does not create a ghost library item"
    );
}
