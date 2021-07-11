# frozen_string_literal: true

module Inkoc
  class UnsignedInteger
    def initialize(value)
      @value
    end

    def value
      @value
    end

    def hash
      @value.hash
    end

    def ==(other)
      other.is_a?(self.class) && value == other.value
    end

    def eql?(other)
      self == other
    end
  end
end
