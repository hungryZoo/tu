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
      sha256 "aca53bc861bfa06cf5c0e7397fea9e279c8f907852f717ff1f86dc88cacca8ae"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.0/tu-1.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "11b89bbe9ae2abdfc670ed720e43461814b54503b78804cc8b03331003a7f875"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.0/tu-1.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "8472f3baddfc0ca1227373133627802dc8ef14812c88d6e68057e16f2209adc6"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.0/tu-1.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "53b2978f46a4e1475ff570a08d62c74586a20e29e2c29fcd760cacce89307208"
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
