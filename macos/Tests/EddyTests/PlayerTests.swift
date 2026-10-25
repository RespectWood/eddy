import Testing
@testable import Eddy

/// Reading the song that TIDAL or Music plays (Copy Link for Current Song).
struct PlayerTests {
    @Test func readsTidalsWindowTitle() {
        let song = Player.songFromWindowTitle("Yeah! (feat. Lil Jon & Ludacris) - USHER, Lil Jon, Ludacris")
        #expect(song?.title == "Yeah! (feat. Lil Jon & Ludacris)")
        #expect(song?.artists == ["USHER", "Lil Jon", "Ludacris"])
    }

    /// The artists come after the last " - "; earlier ones belong to the title.
    @Test func titleWithADash() {
        let song = Player.songFromWindowTitle("Bohemian Rhapsody - Remastered 2011 - Queen")
        #expect(song?.title == "Bohemian Rhapsody - Remastered 2011")
        #expect(song?.artists == ["Queen"])
    }

    @Test(arguments: ["TIDAL", "", " - USHER", "Yeah! - ", "Yeah! -  , "])
    func nothingPlaying(title: String) {
        #expect(Player.songFromWindowTitle(title) == nil)
    }

    @Test func splitsMusicsArtistLine() {
        #expect(Player.splitArtists("USHER feat. Lil Jon & Ludacris") == ["USHER", "Lil Jon", "Ludacris"])
        #expect(Player.splitArtists("Mark Ronson ft. Bruno Mars") == ["Mark Ronson", "Bruno Mars"])
        #expect(Player.splitArtists("Rick Astley") == ["Rick Astley"])
        // Known limitation: duos named with "&" are split too. The primary artist still
        // comes first, which is what the lookups use.
        #expect(Player.splitArtists("Simon & Garfunkel") == ["Simon", "Garfunkel"])
    }

    @Test func readsClockTimes() {
        #expect(TidalPage.milliseconds(fromClock: "4:10") == 250_000)
        #expect(TidalPage.milliseconds(fromClock: "1:02:03") == 3_723_000)
        #expect(TidalPage.milliseconds(fromClock: "0:07") == 7_000)
        #expect(TidalPage.milliseconds(fromClock: "") == nil)
        #expect(TidalPage.milliseconds(fromClock: "4:1x") == nil)
        #expect(TidalPage.milliseconds(fromClock: "-1:00") == nil)
    }
}

/// Autoplay checks that the right song plays before and after pressing Play.
struct AutoPlayTests {
    @Test func comparesTitlesLoosely() {
        #expect(Titles.same("Yeah! (feat. Lil Jon & Ludacris)", "Yeah!"))
        #expect(Titles.same("Bohemian Rhapsody - Remastered 2011", "bohemian rhapsody"))
        #expect(Titles.same("Rock & Roll", "Rock and Roll"))
        // Straight and curly apostrophes, as the services write them differently.
        #expect(Titles.same("Don't Stop [Live]", "Don’t Stop"))
        #expect(!Titles.same("Yeah!", "Yeah Yeah Yeah"))
        #expect(!Titles.same("", ""))
        #expect(!Titles.same("(Intro)", "(Outro)"), "nothing left to compare")
    }

    @Test func readsMusicCatalogIDs() {
        let track = MusicPage.catalogIDs("music://music.apple.com/se/album/confessions/386153429?i=386153478")
        #expect(track?.album == "386153429")
        #expect(track?.track == "386153478")
        let album = MusicPage.catalogIDs("music://music.apple.com/us/album/the-dark-side-of-the-moon/1065973699")
        #expect(album?.album == "1065973699")
        #expect(album?.track == nil)
        #expect(MusicPage.catalogIDs("music://music.apple.com/de/search?term=x") == nil)
        #expect(MusicPage.catalogIDs("") == nil)
    }
}
