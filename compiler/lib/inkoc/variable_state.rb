# frozen_string_literal: true

module Inkoc
  class VariableState
    def initialize
      @moved = false
    end

    def move
      @moved = true
    end

    def unmove
      @moved = false
    end

    def moved?
      @moved
    end
  end
end
