//! Signs in against a real alternative session server (Drasl). Runs only with
//! `MELON_DRASL_TEST="<address>|<player name>|<password>"`, for example a local Drasl at
//! `http://localhost:25585`.

use melon_base::auth::alt_sessions;
use melon_base::auth::minecraft;
use melon_base::auth::skin::{self, SkinSource};

fn server() -> Option<(String, String, String)> {
    let value = std::env::var("MELON_DRASL_TEST").ok()?;
    let mut parts = value.splitn(3, '|').map(str::to_owned);
    Some((parts.next()?, parts.next()?, parts.next()?))
}

/// A 64x64 skin with a red face, PNG-encoded.
fn red_face_skin() -> Vec<u8> {
    let mut image = image::RgbaImage::from_pixel(64, 64, image::Rgba([40, 40, 40, 255]));
    for y in 8..16 {
        for x in 8..16 {
            image.put_pixel(x, y, image::Rgba([200, 20, 20, 255]));
            // No hat over the face.
            image.put_pixel(x + 32, y, image::Rgba([0, 0, 0, 0]));
        }
    }
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

/// authlib-injector's texture upload: `PUT /api/user/profile/<uuid>/skin`, multipart.
fn upload_skin(root: &str, access_token: &str, uuid: &str, png: &[u8]) {
    let boundary = "melon-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let response = ureq::put(&format!("{root}/api/user/profile/{uuid}/skin"))
        .header("Authorization", format!("Bearer {access_token}"))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body[..])
        .expect("skin upload");
    assert!(
        response.status().is_success(),
        "skin upload: {}",
        response.status()
    );
}

#[test]
fn a_drasl_account_signs_in_joins_and_has_its_skin() {
    let Some((address, login, password)) = server() else {
        eprintln!("MELON_DRASL_TEST not set: skipped");
        return;
    };
    let provider = alt_sessions::resolve(&address).expect("resolve");
    assert!(
        provider.root.ends_with("/authlib-injector"),
        "{}",
        provider.root
    );

    let wrong = alt_sessions::authenticate(
        &provider,
        &login,
        "not the password",
        &alt_sessions::new_client_token(),
    );
    assert!(wrong.is_err(), "a wrong password is refused");

    let client = alt_sessions::new_client_token();
    let session =
        alt_sessions::authenticate(&provider, &login, &password, &client).expect("authenticate");
    let profile = session
        .selected_profile
        .clone()
        .expect("one player is selected");
    assert_eq!(profile.name, login);
    assert!(
        alt_sessions::validate(&provider, &session.access_token, &session.client_token).unwrap()
    );

    let fresh = alt_sessions::refresh(
        &provider,
        &session.access_token,
        &session.client_token,
        None,
    )
    .expect("refresh");
    assert_ne!(
        fresh.access_token, session.access_token,
        "refresh hands out a new token"
    );
    assert!(alt_sessions::validate(&provider, &fresh.access_token, &fresh.client_token).unwrap());
    assert!(
        !alt_sessions::validate(&provider, "not-a-token", &fresh.client_token).unwrap(),
        "a made-up token is refused, not an error"
    );

    // Joining a server: the client calls join, the server asks hasJoined.
    let server_id = format!("melon{}", std::process::id());
    minecraft::join_server(
        &provider.session_base(),
        &fresh.access_token,
        &profile.id,
        &server_id,
    )
    .expect("join");
    let url = format!(
        "{}/session/minecraft/hasJoined?username={}&serverId={server_id}",
        provider.session_base(),
        profile.name
    );
    let mut answer = ureq::get(&url).call().expect("hasJoined");
    assert_eq!(answer.status().as_u16(), 200, "the server sees the join");
    let joined: serde_json::Value = answer.body_mut().read_json().unwrap();
    assert_eq!(
        joined["id"].as_str().unwrap().replace('-', ""),
        profile.id.replace('-', "")
    );

    upload_skin(
        &provider.root,
        &fresh.access_token,
        &profile.id,
        &red_face_skin(),
    );
    let started = std::time::Instant::now();
    let head = skin::fetch_head_from(&SkinSource::of(&provider), &profile.id)
        .expect("head from the service");
    eprintln!("head fetched in {:?}", started.elapsed());
    assert_eq!(head.get_pixel(0, 0).0, [200, 20, 20, 255]);

    alt_sessions::invalidate(&provider, &fresh.access_token, &fresh.client_token);
    assert!(!alt_sessions::validate(&provider, &fresh.access_token, &fresh.client_token).unwrap());
}

/// Signs in to every preset service with one account and changes nothing there. Runs only with
/// `MELON_ALT_ACCOUNT="<email or name>|<password>|<user name, for services that take no email>"`.
#[test]
fn the_preset_services_sign_in_and_join() {
    let Ok(value) = std::env::var("MELON_ALT_ACCOUNT") else {
        eprintln!("MELON_ALT_ACCOUNT not set: skipped");
        return;
    };
    let mut parts = value.splitn(3, '|');
    let (login, password) = (parts.next().unwrap(), parts.next().expect("login|password"));
    let name = parts.next().unwrap_or(login);
    let mut failures = Vec::new();
    for preset in &alt_sessions::PRESETS {
        let result = (|| -> anyhow::Result<String> {
            let provider = alt_sessions::resolve(preset.address)?;
            let client = alt_sessions::new_client_token();
            let login = if preset.name_only { name } else { login };
            let session = alt_sessions::authenticate(&provider, login, password, &client)?;
            let profiles: Vec<_> = session
                .available_profiles
                .iter()
                .map(|p| p.name.clone())
                .collect();
            let Some(profile) = session.selected_profile.clone() else {
                alt_sessions::invalidate(&provider, &session.access_token, &session.client_token);
                return Ok(format!(
                    "signed in, no player selected; players: {profiles:?}"
                ));
            };
            anyhow::ensure!(alt_sessions::validate(
                &provider,
                &session.access_token,
                &session.client_token
            )?);
            let fresh = alt_sessions::refresh(
                &provider,
                &session.access_token,
                &session.client_token,
                None,
            )?;
            anyhow::ensure!(alt_sessions::validate(
                &provider,
                &fresh.access_token,
                &fresh.client_token
            )?);
            let server_id = format!("melon{}", std::process::id());
            minecraft::join_server(
                &provider.session_base(),
                &fresh.access_token,
                &profile.id,
                &server_id,
            )?;
            let url = format!(
                "{}/session/minecraft/hasJoined?username={}&serverId={server_id}",
                provider.session_base(),
                profile.name
            );
            let status = ureq::get(&url).call()?.status().as_u16();
            anyhow::ensure!(status == 200, "hasJoined answered {status}");
            let head = match skin::fetch_head_from(&SkinSource::of(&provider), &profile.id) {
                Ok(_) => "head loaded".to_owned(),
                Err(e) => format!("no head ({e:#})"),
            };
            alt_sessions::invalidate(&provider, &fresh.access_token, &fresh.client_token);
            Ok(format!(
                "player {}: sign-in, validate, refresh, join and hasJoined work; {head}",
                profile.name
            ))
        })();
        match result {
            Ok(report) => eprintln!("{}: {report}", preset.name),
            Err(e) => {
                eprintln!("{}: FAILED: {e:#}", preset.name);
                failures.push(preset.name);
            }
        }
    }
    assert!(failures.is_empty(), "failed: {failures:?}");
}
