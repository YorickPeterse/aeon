# frozen_string_literal: true

module Inkoc
  module AST
    class Break
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :location

      def initialize(location)
        @location = location
      end

      def visitor_method
        :on_break
      end
    end
  end
end
