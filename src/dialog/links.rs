//! The links a server lists for its players (vanilla `ServerLinks`): its
//! website, rules, bug tracker..., shown by the `server_links` dialog.

use anyhow::Result;

use crate::component::{Component, is_trusted_url};
use crate::network::protocol::reader::{Reader, initial_capacity};

/// `ServerLinks.KnownLinkType`, by its wire id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownLink {
    BugReport,
    CommunityGuidelines,
    Support,
    Status,
    Feedback,
    Community,
    Website,
    Forums,
    News,
    Announcements,
}

impl KnownLink {
    const ALL: [Self; 10] = [
        Self::BugReport,
        Self::CommunityGuidelines,
        Self::Support,
        Self::Status,
        Self::Feedback,
        Self::Community,
        Self::Website,
        Self::Forums,
        Self::News,
        Self::Announcements,
    ];

    /// `ByIdMap.continuous` with `OutOfBoundsStrategy.ZERO`.
    fn by_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(Self::BugReport)
    }

    fn name(self) -> &'static str {
        match self {
            Self::BugReport => "report_bug",
            Self::CommunityGuidelines => "community_guidelines",
            Self::Support => "support",
            Self::Status => "status",
            Self::Feedback => "feedback",
            Self::Community => "community",
            Self::Website => "website",
            Self::Forums => "forums",
            Self::News => "news",
            Self::Announcements => "announcements",
        }
    }
}

/// `ServerLinks.Entry`: a known kind or the server's own name, and a link.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerLink {
    pub kind: Result<KnownLink, Component>,
    pub url: String,
}

impl ServerLink {
    /// `Entry.displayName`.
    pub fn display_name(&self) -> Component {
        match &self.kind {
            Ok(known) => {
                Component::translatable(format!("known_server_link.{}", known.name()), Vec::new())
            }
            Err(name) => name.clone(),
        }
    }
}

/// `ClientboundServerLinksPacket`, with `handleServerLinks`' check: links
/// that are not http(s) are dropped.
pub fn read(body: &[u8]) -> Result<Vec<ServerLink>> {
    let mut r = Reader::new(body);
    let count = r.list_count()?;
    let mut links = Vec::with_capacity(initial_capacity(count));
    for _ in 0..count {
        let kind = if r.bool()? {
            Ok(KnownLink::by_id(r.var_int()?))
        } else {
            Err(r.component()?)
        };
        let url = r.string()?;
        if is_trusted_url(&url) {
            links.push(ServerLink { kind, url });
        } else {
            crate::warn!("Server link with an invalid address dropped: {url}");
        }
    }
    Ok(links)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::{reader, varint};

    #[test]
    fn known_and_named_links_are_read_and_bad_ones_dropped() {
        let mut body = Vec::new();
        varint::write_var_int(&mut body, 3).unwrap();
        body.push(1);
        varint::write_var_int(&mut body, 6).unwrap();
        reader::write_string(&mut body, "https://example.com").unwrap();
        body.push(1);
        varint::write_var_int(&mut body, 99).unwrap();
        reader::write_string(&mut body, "javascript:alert(1)").unwrap();
        body.push(0);
        // A string tag: the text component "Shop".
        body.extend_from_slice(&[8, 0, 4]);
        body.extend_from_slice(b"Shop");
        reader::write_string(&mut body, "http://shop.example.com").unwrap();
        let links = read(&body).unwrap();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].kind, Ok(KnownLink::Website));
        assert_eq!(links[1].display_name().plain_text(), "Shop");
    }
}
