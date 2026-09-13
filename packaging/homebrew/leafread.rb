# Homebrew formula for leafread.
#
# Maintainer: copy to cigan1/homebrew-tap as Formula/leafread.rb, then fill in
# `version` and the four `sha256` values from the GitHub Release checksums.txt.
# See RELEASING.md.

class Leafread < Formula
  desc "Terminal Markdown reader with rich formatting, search, and images"
  homepage "https://github.com/cigan1/leafread"
  version "0.1.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/cigan1/leafread/releases/download/v#{version}/leafread-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
    on_intel do
      url "https://github.com/cigan1/leafread/releases/download/v#{version}/leafread-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/cigan1/leafread/releases/download/v#{version}/leafread-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
    on_intel do
      url "https://github.com/cigan1/leafread/releases/download/v#{version}/leafread-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
  end

  def install
    bin.install "leafread"
  end

  test do
    (testpath/"test.md").write("# Hello\n\nA **bold** line.\n")
    assert_match "Hello", shell_output("#{bin}/leafread --no-tui --no-color test.md")
  end
end
