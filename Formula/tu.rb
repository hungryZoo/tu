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
  version "1.1.1"
  license "MIT"

  depends_on "tmux"

  on_macos do
    on_arm do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.1/tu-1.1.1-aarch64-apple-darwin.tar.gz"
      sha256 "b9b3fca05fc44d47503ec5a54d75449aac2acd35dc22ce45b803f4c31b5613af"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.1/tu-1.1.1-x86_64-apple-darwin.tar.gz"
      sha256 "b2d242391a95d0bd127aa90b8e2bd5307ad132b3e40da95b3cde7b0c290cc08a"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.1/tu-1.1.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "c3f901cc7fc60b25155a55a823481a4e1488f3d689e1934ffa9eb40f3e6e8632"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.1.1/tu-1.1.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "d22abcdf5d0b3cbc237ab812cc844b17b21f73fdab6265f8809e79affb85b8cd"
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
