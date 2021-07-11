# frozen_string_literal: true

module Inkoc
  module TIR
    class VirtualRegister
      include Inspect

      attr_reader :id, :type

      def self.reserved
        # Register 0 is reserved and used for padding missing optional
        # arguments.
        new(0, TypeSystem::Any.new)
      end

      def initialize(id, type)
        @id = id
        @type = type
      end
    end
  end
end
