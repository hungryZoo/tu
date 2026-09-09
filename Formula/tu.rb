# typed: false
# frozen_string_literal: true

# Homebrew formula for `tu`.
#
# This file lives at Formula/tu.rb in the project repo itself, so users
# can tap and install with:
#
#   brew tap hungryZoo/tu https://github.com/hungryZoo/tu
#   brew install tu
#
# Bottles are not built; the formula points straight at the
# pre-compiled binaries that are attached to every GitHub release.
class Tu < Formula
  desc "Tiny TUI menu on top of tmux"
  homepage "https://github.com/hungryZoo/tu"
  version "1.1.0"
  license "MIT"

  depends_on "tmux"

  on_macos do
    on_arm do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.0/tu-1.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "12382361773e841eec1eb2e099d35d109df9c3403c95947a6815239715bff727"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.0/tu-1.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "f6bcc31b71b6ac86900b9089206d4a694d9d9093e8c64f2bf89119d33e0cdf4a"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.0/tu-1.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "38faa1f289459c09196ec4a07e7ab4e2b4ed56a8323a0139591c2d9dbb507504"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.0/tu-1.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e346ed4da7ec0acf555203fc99bbc73b72768e85745808b2b3a6b6ebdad16371"
    end
  end

  def install
    bin.install "tu"
    doc.install "README.md", "LICENSE"
  end

  test do
    assert_match "tu #{version}", shell_output("#{bin}/tu --version")
  end
end
