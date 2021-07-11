# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetLocal
        include Predicates
        include Inspect

        attr_reader :variable, :value, :location

        def initialize(variable, value, location)
          @variable = variable
          @value = value
          @location = location
        end

        def register
          nil
        end

        def visitor_method
          :on_set_local
        end
      end
    end
  end
end
