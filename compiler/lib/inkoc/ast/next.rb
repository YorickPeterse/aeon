# frozen_string_literal: true

module Inkoc
  module AST
    class Next
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :location

      def initialize(location)
        @location = location
      end

      def visitor_method
        :on_next
      end
    end
  end
end
